# Tatar

[English](README.en.md) · **Türkçe**

## Tatar nedir?

Tatar, Android ve iPhone telefonunuzu Mac veya Windows bilgisayarınıza bağlayan hafif bir masaüstü uygulamasıdır. Adını, Osmanlı'da menzilden menzile haber taşıyan posta kuryelerinden (tatar) alır.

Tatar, [KDE Connect](https://kdeconnect.kde.org/) protokolünü konuşur. Telefonunuzdaki **resmî KDE Connect uygulaması hiç değiştirilmeden** çalışır; telefon tarafına bir şey kurmanız gerekmez. Tatar, KDE Connect masaüstü uygulamasının bir çatalı (fork) değildir; protokolü baştan yazılmış bağımsız bir istemcidir.

macOS'ta menü çubuğunda yaşar, Windows'ta sistem tepsisinde durur. Tauri 2, React ve Rust ile yazılmıştır.

## Özellikler (v1)

- **Pil durumu:** Telefonun şarj yüzdesini ve şarj durumunu gösterir. Pil %15'in altına inince ve tam dolunca bildirim çıkarır.
- **Ping ve telefonu bul:** Telefona ping gönderir ya da telefonu çaldırır.
- **Pano senkronizasyonu:** Otomatik, elle veya kapalı çalışır. En fazla 20 öğelik pano geçmişi tutar. Şifre yöneticisi gibi gizli olarak işaretlenmiş içerik asla gönderilmez ve geçmişe yazılmaz.
- **Dosya gönderme (iki yön):** Dosyaları sürükleyip bırakarak ya da seçerek telefona gönderebilir, telefondan alabilirsiniz. İlerleme çubuğu, iptal ve "Tekrar dene" seçenekleri vardır.
- **Arama gelince sesi kapat:** Telefon çaldığında bilgisayarın sesini kapatır veya azaltır, arama bitince önceki sesi geri yükler.
- **Genel kısayollar:**
  - ⇧⌘C: Panoyu telefona gönder
  - ⇧⌘F: Dosya gönder
  - ⌥⌘R: Telefonu çaldır
  - ⇧⌘V: Pano geçmişini aç
  - Windows'ta ⌘ yerine Ctrl, ⌥ yerine Alt kullanılır (örneğin Ctrl+Alt+R).
- **Sorun giderme ekranı:** Ağ, izin ve eşleşme sorunlarını gerçek kontrollerle tek tek denetler.
- **Türkçe ve İngilizce arayüz:** Varsayılan olarak sistem dilini kullanır, Ayarlar'dan değiştirilebilir.
- **Erişilebilirlik:** "Saydamlığı azalt" ayarını ve "Hareketi azalt" tercihini izler.

Bu sürümde olmayanlar (uzaktan giriş, SMS, medya kontrolü, dosya gezgini, komut çalıştırma, bildirim yansıtma ve benzeri) [docs/ROADMAP.md](docs/ROADMAP.md) dosyasında listelenmiştir.

## Kurulum

Tatar henüz resmî bir sürüm olarak yayınlanmadı. Derlenmiş dosyalar GitHub Actions üzerinden alınır.

### macOS (macOS 13 ve üzeri, Apple Silicon ve Intel)

1. GitHub'da depoyu açın ve **Actions** sekmesine gidin.
2. Soldaki listeden **build** iş akışını seçin ve en son başarılı (yeşil) çalıştırmayı açın.
3. Sayfanın altındaki **Artifacts** bölümünden **Tatar-macos** dosyasını indirin. İçinden `Tatar.app.zip` ve `.dmg` dosyası çıkar.
4. `Tatar.app` uygulamasını **Uygulamalar** klasörüne taşıyın.

**İlk açılış.** Uygulama Apple tarafından imzalanmamıştır, bu yüzden macOS ilk açılışta uyarır:

- `Tatar.app` üzerine **sağ tıklayın** ve **Aç**'ı seçin, açılan pencerede yine **Aç**'a basın.
- macOS 15 ve üzerinde bu yol işe yaramazsa **Sistem Ayarları → Gizlilik ve Güvenlik** bölümüne gidin, aşağıda çıkan **"Yine de Aç"** düğmesine basın.

**İzinler.**

- **Yerel ağ erişimi:** Uygulama ilk kez açıldığında izin isterse **İzin ver**'i seçin. Yoksa telefon bulunamaz.
- **Anahtar zinciri (Keychain):** Güvenli bağlantı için kullanılan özel anahtar macOS anahtar zincirinde saklanır. Parola istendiğinde Mac'inizin giriş parolasını girin ve **Her Zaman İzin Ver** seçeneğini seçin. İmzasız sürümler her güncellemeden sonra bu soruyu yeniden sorar.

### Windows (Windows 10 ve 11, x64)

1. Yukarıdaki adımlarla aynı şekilde **Tatar-windows** dosyasını indirin.
2. İçinden `*-setup.exe` (veya `.msi`) dosyasını çalıştırın.
3. **Windows SmartScreen** uyarısı çıkarsa **Ek bilgi**'ye, sonra **Yine de çalıştır**'a tıklayın. Uygulama imzasız olduğu için bu uyarı normaldir.
4. Güvenlik duvarı izin penceresi çıkarsa **Özel ağlar** için izin verin. Yalnızca evdeki veya işteki ağ için izin vermek yeterlidir.

## Eşleştirme

1. Telefon ve bilgisayar **aynı Wi-Fi ağında** olmalı.
2. Telefonunuzda **KDE Connect** uygulamasını açın. Listede **Tatar** ile birlikte bilgisayarınızın adı görünmelidir.
3. Bilgisayarın adına dokunun ve **Eşleştir**'e basın.
4. Telefonda ve bilgisayarda aynı **8 karakterlik kod** görünür. İki ekrandaki kodun birebir aynı olduğunu kontrol edin.
5. Her iki tarafta da **Kabul et** deyin.

Bilgisayar listede görünmüyorsa Tatar'ın **Sorun giderme** ekranında **IP adresiyle elle ekle** seçeneğini kullanın ve telefonun IP adresini girin.

**Güvenlik duvarı ve bağlantı noktaları.** Telefon ile bilgisayar arasındaki trafiğe şu portların açık olması gerekir: **UDP ve TCP 1714–1764**. Bazı ağlar (özellikle misafir ağları) cihazların birbirini görmesini engeller. Bu durumda ağ yöneticinizden izin isteyin ya da cihazları aynı özel ağa bağlayın.

## Geliştirme

### Gereksinimler

- **Xcode Command Line Tools:** `xcode-select --install`
- **Rust:** [rustup](https://rustup.rs/) ile kurulur
- **Node.js LTS** (sürüm 22 önerilir)

### Komutlar

```bash
# Bağımlılıkları kur
npm ci

# Geliştirme modunda çalıştır (kod değiştikçe yenilenir)
npm run tauri dev

# Paketlenmiş uygulamayı derleyip aç (macOS)
npm run app

# Rust testleri
cd src-tauri && cargo test

# Biçim ve kod denetimleri
npm run typecheck      # TypeScript tür denetimi
npm run lint           # ESLint
npm run format:check   # Prettier biçim denetimi
cd src-tauri
cargo fmt --check      # Rust biçim denetimi
cargo clippy --all-targets -- -D warnings
```

`npm run app` önce çalışan Tatar'ı kapatır, uygulamayı derler ve `src-tauri/target/release/bundle/macos/Tatar.app` dosyasını açar.

Derleme ve testler her gönderimde GitHub Actions üzerinde de çalışır (`.github/workflows/build.yml`).

## Gizlilik ve güvenlik

- **Özel anahtar** yalnızca işletim sistemi anahtar zincirinde (macOS Keychain, Windows Credential Manager) saklanır. Düz bir dosyaya yazılmaz.
- **Gizli pano içeriği** (şifre yöneticilerinin işaretlediği metinler) hiçbir zaman telefona gönderilmez ve pano geçmişine yazılmaz.
- **Pano geçmişi** yalnızca bellekte tutulur, en fazla 20 öğedir. **Temizle** düğmesi hepsini siler.
- **Eşleşmemiş cihazlar** yalnızca eşleştirme isteği gönderebilir. Eşleşmiş bir cihazın sertifikası değişirse bağlantı reddedilir.
- **Eşleştirme isteği** siz onaylamadan kabul edilmez. Doğrulama kodu her zaman gösterilir.
- **Gelen dosyalar** yalnızca seçtiğiniz indirme klasörüne yazılır. Dosya adındaki yol karakterleri temizlenir, aynı adlı dosyanın üzerine yazılmaz (`ad (1).uzantı` olarak kaydedilir).
- **Ağdan gelen her veri** güvenilmez kabul edilir: boyut sınırı vardır, bilinmeyen paketler yok sayılır.
- **Loglara** pano içeriği, dosya içeriği ya da anahtar yazılmaz. "Tanılama günlüğünü kopyala" seçeneği de bu bilgileri içermez.

## Lisans

Henüz karar verilmedi.
