//! rustls yapılandırması. Sağlayıcı: `ring` (Windows'ta NASM/cmake gerektirmez; ECDSA P-256
//! ve RSA el sıkışma imzalarını doğrular. Sertifikanın KENDİ imzası (SHA-512) doğrulanmaz,
//! çünkü zincir/CA doğrulaması yapmıyoruz; yalnızca el sıkışma imzası ve sabitleme önemli).
//!
//! Kontrol bağlantısında roller TERS: TCP'yi açan taraf TLS sunucusu, kabul eden TLS istemcisi.
//! Güven kuralı: eşleşmemiş eşin sertifikası kabul edilir (sonra sabitlenir); eşleşmiş eşin
//! sertifikası saklanan DER ile BİREBİR aynı olmak zorunda, aksi halde el sıkışma düşer.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms, ring as ring_provider};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, DistinguishedName, Error, ServerConfig,
    SignatureScheme,
};

use super::cert::LocalIdentity;

fn provider() -> Arc<CryptoProvider> {
    Arc::new(ring_provider::default_provider())
}

fn algs() -> WebPkiSupportedAlgorithms {
    ring_provider::default_provider().signature_verification_algorithms
}

/// Eşleşmiş eş için sabitlenmiş DER; yoksa herhangi bir sertifika kabul.
fn check_pin(pinned: &Option<Vec<u8>>, presented: &CertificateDer<'_>) -> Result<(), Error> {
    match pinned {
        Some(p) if p.as_slice() != presented.as_ref() => Err(Error::InvalidCertificate(
            CertificateError::ApplicationVerificationFailure,
        )),
        _ => Ok(()),
    }
}

#[derive(Debug)]
struct PinVerifier {
    pinned: Option<Vec<u8>>,
}

impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, Error> {
        check_pin(&self.pinned, end_entity)?;
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &algs())
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &algs())
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        algs().supported_schemes()
    }
}

#[derive(Debug)]
struct PinClientVerifier {
    pinned: Option<Vec<u8>>,
}

impl ClientCertVerifier for PinClientVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }
    fn client_auth_mandatory(&self) -> bool {
        // Eşleşmiş cihazda zorunlu; eşleşmemişte isteğe bağlı (sonradan CN kontrolü yine yapılır).
        self.pinned.is_some()
    }
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }
    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        check_pin(&self.pinned, end_entity)?;
        Ok(ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &algs())
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &algs())
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        algs().supported_schemes()
    }
}

fn own_cert(local: &LocalIdentity) -> (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>) {
    (
        vec![CertificateDer::from(local.cert_der.clone())],
        PrivateKeyDer::from(PrivatePkcs8KeyDer::from(local.key_der.clone())),
    )
}

/// TLS SUNUCU yapılandırması (kontrol bağlantısını AÇAN taraf bunu kullanır).
/// TLS 1.2 ve 1.3; istemci sertifikası her zaman istenir.
pub fn server_config(
    local: &LocalIdentity,
    pinned: Option<Vec<u8>>,
) -> Result<Arc<ServerConfig>, String> {
    let (chain, key) = own_cert(local);
    let cfg = ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .with_client_cert_verifier(Arc::new(PinClientVerifier { pinned }))
        .with_single_cert(chain, key)
        .map_err(|e| e.to_string())?;
    Ok(Arc::new(cfg))
}

/// TLS İSTEMCİ yapılandırması (kontrol bağlantısını KABUL EDEN taraf bunu kullanır).
pub fn client_config(
    local: &LocalIdentity,
    pinned: Option<Vec<u8>>,
) -> Result<Arc<ClientConfig>, String> {
    let (chain, key) = own_cert(local);
    let cfg = ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinVerifier { pinned }))
        .with_client_auth_cert(chain, key)
        .map_err(|e| e.to_string())?;
    Ok(Arc::new(cfg))
}

/// Alan adı doğrulanmaz; deviceId'ler `_` içerebildiğinden sabit bir yer tutucu kullanılır.
pub fn dummy_server_name() -> ServerName<'static> {
    ServerName::try_from("kdeconnect.local").expect("sabit ad geçerli")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::cert::{LocalIdentity, MemoryKeyStore};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_rustls::{TlsAcceptor, TlsConnector};

    fn ident(tag: &str) -> LocalIdentity {
        let dir = std::env::temp_dir().join(format!(
            "tatar-tls-{tag}-{}",
            crate::protocol::identity::generate_device_id()
        ));
        let l = LocalIdentity::load_or_create(&dir, &MemoryKeyStore::default()).unwrap();
        let _ = std::fs::remove_dir_all(dir);
        l
    }

    /// Android yalnızca TLS 1.2 konuşur: istemci 1.2'ye kilitliyken de el sıkışma tamamlanmalı.
    async fn run(
        pin_client_to: Option<Vec<u8>>,
        server_pin: Option<Vec<u8>>,
        tls12_only: bool,
    ) -> bool {
        let (srv_id, cli_id) = (ident("s"), ident("c"));
        let scfg = server_config(&srv_id, server_pin).unwrap();
        let ccfg = if tls12_only {
            let (chain, key) = own_cert(&cli_id);
            Arc::new(
                ClientConfig::builder_with_provider(provider())
                    .with_protocol_versions(&[&rustls::version::TLS12])
                    .unwrap()
                    .dangerous()
                    .with_custom_certificate_verifier(Arc::new(PinVerifier {
                        pinned: pin_client_to,
                    }))
                    .with_client_auth_cert(chain, key)
                    .unwrap(),
            )
        } else {
            client_config(&cli_id, pin_client_to).unwrap()
        };
        let (a, b) = tokio::io::duplex(16 * 1024);
        let server = tokio::spawn(async move {
            let mut s = TlsAcceptor::from(scfg).accept(a).await?;
            let mut buf = [0u8; 4];
            s.read_exact(&mut buf).await?;
            s.write_all(&buf).await?;
            s.flush().await?;
            Ok::<_, std::io::Error>(s.get_ref().1.protocol_version())
        });
        let client = async {
            let mut c = TlsConnector::from(ccfg)
                .connect(dummy_server_name(), b)
                .await?;
            c.write_all(b"ping").await?;
            let mut buf = [0u8; 4];
            c.read_exact(&mut buf).await?;
            Ok::<_, std::io::Error>(c.get_ref().1.protocol_version())
        };
        let cres = client.await;
        let sres = server.await.unwrap();
        if tls12_only && let Ok(v) = &cres {
            assert_eq!(*v, Some(rustls::ProtocolVersion::TLSv1_2));
        }
        cres.is_ok() && sres.is_ok()
    }

    #[tokio::test]
    async fn tls12_and_tls13_handshake_unpaired() {
        assert!(run(None, None, true).await);
        assert!(run(None, None, false).await);
    }

    #[tokio::test]
    async fn pinned_certificate_mismatch_fails() {
        let other = ident("o");
        // İstemci, sunucunun sertifikasını başkasının DER'ine sabitlemiş: el sıkışma düşmeli.
        assert!(!run(Some(other.cert_der.clone()), None, false).await);
        // Sunucu, istemci sertifikasını başkasına sabitlemiş: düşmeli.
        assert!(!run(None, Some(other.cert_der.clone()), false).await);
    }
}
