//! Kendinden imzalı cihaz sertifikası. Özel anahtar yalnızca işletim sistemi anahtar
//! zincirinde durur (macOS Keychain / Windows Credential Manager); sertifika (genel) dosyada.

use std::path::Path;
use std::sync::Mutex;

use rcgen::{CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, PKCS_ECDSA_P256_SHA256};
use x509_parser::prelude::*;

use super::identity::{generate_device_id, is_valid_device_id};

/// Özel anahtar deposu. Testler bellek içi sürümünü kullanır, gerçek anahtar zincirine dokunmaz.
pub trait KeyStore: Send + Sync {
    fn load(&self) -> Result<Option<String>, String>;
    fn store(&self, pkcs8_pem: &str) -> Result<(), String>;
    fn delete(&self) -> Result<(), String>;
}

pub struct KeyringStore {
    service: String,
    user: String,
}

impl KeyringStore {
    pub fn new(service: &str, user: &str) -> Self {
        KeyringStore {
            service: service.into(),
            user: user.into(),
        }
    }
    fn entry(&self) -> Result<keyring::Entry, String> {
        keyring::Entry::new(&self.service, &self.user).map_err(|e| e.to_string())
    }
}

impl KeyStore for KeyringStore {
    fn load(&self) -> Result<Option<String>, String> {
        match self.entry()?.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn store(&self, pem: &str) -> Result<(), String> {
        self.entry()?.set_password(pem).map_err(|e| e.to_string())
    }
    fn delete(&self) -> Result<(), String> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[derive(Default)]
pub struct MemoryKeyStore(Mutex<Option<String>>);

impl KeyStore for MemoryKeyStore {
    fn load(&self) -> Result<Option<String>, String> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn store(&self, pem: &str) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(pem.to_string());
        Ok(())
    }
    fn delete(&self) -> Result<(), String> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}

/// Bu cihazın kimliği: id + sertifika (DER) + özel anahtar (PKCS#8 DER).
pub struct LocalIdentity {
    pub device_id: String,
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
    /// Sertifika/anahtar ilk kez ya da yeniden üretildi (eski güvenilir cihazlar geçersiz).
    pub regenerated: bool,
}

impl std::fmt::Debug for LocalIdentity {
    // Anahtar asla loga düşmesin.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalIdentity")
            .field("device_id", &self.device_id)
            .finish_non_exhaustive()
    }
}

const ID_FILE: &str = "device_id";
const CERT_FILE: &str = "certificate.der";

/// Sertifikadan özne CN'ini okur.
pub fn common_name(cert_der: &[u8]) -> Option<String> {
    let (_, cert) = X509Certificate::from_der(cert_der).ok()?;
    cert.subject()
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .map(str::to_string)
}

/// SubjectPublicKeyInfo DER (doğrulama kodu için; tüm sertifika değil).
pub fn spki_der(cert_der: &[u8]) -> Option<Vec<u8>> {
    let (_, cert) = X509Certificate::from_der(cert_der).ok()?;
    Some(cert.tbs_certificate.subject_pki.raw.to_vec())
}

fn validity_ok(cert_der: &[u8]) -> bool {
    X509Certificate::from_der(cert_der)
        .map(|(_, c)| c.validity().is_valid())
        .unwrap_or(false)
}

/// EC P-256 sertifika üretir: CN=<id>, OU=KDE Connect, O=KDE, -1 yıl .. +10 yıl.
/// İmza algoritması: `ring` ECDSA-P256'yı SHA-512 ile imzalayamaz, bu yüzden
/// ecdsa-with-SHA256 kullanılır (referans uygulamalar SHA-512). Android'in bunu
/// kabul ettiği gerçek telefonla doğrulanmalı (notlar §11.1).
pub fn generate(device_id: &str) -> Result<(Vec<u8>, Vec<u8>), String> {
    let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).map_err(|e| e.to_string())?;
    let mut params = CertificateParams::new(Vec::<String>::new()).map_err(|e| e.to_string())?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, device_id);
    dn.push(DnType::OrganizationalUnitName, "KDE Connect");
    dn.push(DnType::OrganizationName, "KDE");
    params.distinguished_name = dn;
    params.is_ca = IsCa::ExplicitNoCa;
    let now = ::time::OffsetDateTime::now_utc();
    params.not_before = now - ::time::Duration::days(365);
    params.not_after = now + ::time::Duration::days(365 * 10);
    let serial: [u8; 8] = rand::random();
    // İlk bit 0 kalsın (pozitif tamsayı).
    let mut serial = serial;
    serial[0] &= 0x7f;
    params.serial_number = Some(rcgen::SerialNumber::from_slice(&serial));
    let cert = params.self_signed(&key).map_err(|e| e.to_string())?;
    Ok((cert.der().to_vec(), key.serialize_der()))
}

fn write_private(path: &Path, data: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

impl LocalIdentity {
    /// Varsa yükler; eksik/bozuk/süresi dolmuşsa YENİ id + sertifika üretir
    /// (aynı id ile yeni sertifika, telefondaki eski güveni kırık bırakırdı).
    pub fn load_or_create(dir: &Path, keys: &dyn KeyStore) -> Result<LocalIdentity, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        if let Some(existing) = Self::try_load(dir, keys) {
            return Ok(existing);
        }
        let device_id = generate_device_id();
        let (cert_der, key_der) = generate(&device_id)?;
        let key_pem = KeyPair::try_from(key_der.as_slice())
            .map_err(|e| e.to_string())?
            .serialize_pem();
        keys.store(&key_pem)?;
        write_private(&dir.join(CERT_FILE), &cert_der)?;
        write_private(&dir.join(ID_FILE), device_id.as_bytes())?;
        log::info!("yeni cihaz kimliği ve sertifika üretildi");
        Ok(LocalIdentity {
            device_id,
            cert_der,
            key_der,
            regenerated: true,
        })
    }

    fn try_load(dir: &Path, keys: &dyn KeyStore) -> Option<LocalIdentity> {
        let device_id = std::fs::read_to_string(dir.join(ID_FILE))
            .ok()?
            .trim()
            .to_string();
        let cert_der = std::fs::read(dir.join(CERT_FILE)).ok()?;
        let pem = keys.load().ok()??;
        let key = KeyPair::from_pem(&pem).ok()?;
        if !is_valid_device_id(&device_id)
            || common_name(&cert_der).as_deref() != Some(device_id.as_str())
            || !validity_ok(&cert_der)
        {
            return None;
        }
        // Anahtar sertifikayla eşleşmeli.
        if spki_der(&cert_der)? != rcgen::PublicKeyData::subject_public_key_info(&key) {
            return None;
        }
        Some(LocalIdentity {
            device_id,
            cert_der,
            key_der: key.serialize_der(),
            regenerated: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_cert_fields() {
        let id = generate_device_id();
        let (cert, key) = generate(&id).unwrap();
        assert!(!key.is_empty());
        assert_eq!(common_name(&cert).as_deref(), Some(id.as_str()));
        let (_, c) = X509Certificate::from_der(&cert).unwrap();
        assert!(c.validity().is_valid());
        assert_eq!(c.subject(), c.issuer());
        let o = c
            .subject()
            .iter_organization()
            .next()
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(o, "KDE");
        let ou = c
            .subject()
            .iter_organizational_unit()
            .next()
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(ou, "KDE Connect");
        assert_eq!(spki_der(&cert).unwrap().len(), 91);
        let years =
            (c.validity().not_after.timestamp() - c.validity().not_before.timestamp()) / 86400;
        assert!((4014..=4016).contains(&years), "{years}"); // -1 yıl .. +10 yıl
    }

    #[test]
    fn load_or_create_is_stable() {
        let dir = std::env::temp_dir().join(format!("tatar-cert-{}", generate_device_id()));
        let keys = MemoryKeyStore::default();
        let a = LocalIdentity::load_or_create(&dir, &keys).unwrap();
        assert!(a.regenerated);
        let b = LocalIdentity::load_or_create(&dir, &keys).unwrap();
        assert!(!b.regenerated);
        assert_eq!(a.device_id, b.device_id);
        assert_eq!(a.cert_der, b.cert_der);
        // anahtar kaybolursa yeni kimlik üretilir
        keys.delete().unwrap();
        let c = LocalIdentity::load_or_create(&dir, &keys).unwrap();
        assert!(c.regenerated);
        assert_ne!(a.device_id, c.device_id);
        let _ = std::fs::remove_dir_all(dir);
    }
}
