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
- **Arama gelince müziği duraklat:** Telefon çaldığında bilgisayardaki müziği duraklatır.
- **Bildirim yansıtma:** Telefonunuza gelen bildirimleri bilgisayarda sistem bildirimi olarak görürsünüz. Telefonda KDE Connect'e bildirim erişimi izni verilmiş olmalıdır. Bildirim içeriğini gizlemek de mümkündür.
- **Medya kontrolü:** Telefondaki müziği bilgisayardan oynatabilir, duraklatabilir ve parçaları değiştirebilirsiniz; bilgisayardaki medyayı da telefondan yönetebilirsiniz. Telefonda KDE Connect'e bildirim erişimi izni gerekir. macOS'ta yalnızca Müzik ve Spotify desteklenir.
- **Telefondan komut çalıştırma:** Ayarlar → Cihaz bölümünde tanımladığınız komutları telefonunuzdaki KDE Connect'ten bilgisayarda çalıştırabilirsiniz.
- **Genel kısayollar:**
  - ⇧⌘C: Panoyu telefona gönder
  - ⇧⌘F: Dosya gönder
  - ⌥⌘R: Telefonu çaldır
  - ⇧⌘V: Pano geçmişini aç
  - Windows'ta ⌘ yerine Ctrl, ⌥ yerine Alt kullanılır (örneğin Ctrl+Alt+R).
- **Sorun giderme ekranı:** Ağ, izin ve eşleşme sorunlarını gerçek kontrollerle tek tek denetler.
- **Türkçe ve İngilizce arayüz:** Varsayılan olarak sistem dilini kullanır, Ayarlar'dan değiştirilebilir.
- **Erişilebilirlik:** "Saydamlığı azalt" ayarını ve "Hareketi azalt" tercihini izler.

Bu sürümde olmayanlar (uzaktan giriş, SMS, dosya gezgini ve benzeri) [docs/ROADMAP.md](docs/ROADMAP.md) dosyasında listelenmiştir.

## Kurulum

Kurulum dosyaları GitHub'daki [Sürümler (Releases)](https://github.com/Augustus29/tatar/releases/latest) sayfasındadır.

### macOS (macOS 13 ve üzeri, Apple Silicon ve Intel)

**Kolay yol (önerilen).** **Terminal** uygulamasını açın (Spotlight'ta "Terminal" yazın), aşağıdaki satırı yapıştırıp Enter'a basın:

```bash
curl -fsSL https://raw.githubusercontent.com/Augustus29/tatar/main/scripts/install.sh | sh
```

Bu komut son sürümü indirir, bozulmadığını kontrol eder, **Uygulamalar** klasörüne kurar ve açar. Bu yolla kurulduğunda macOS'un "Tatar açılmadı" uyarısı çıkmaz. Güncellemek için aynı komutu yeniden çalıştırmanız yeterlidir.

**DMG ile kurulum.** [Sürümler](https://github.com/Augustus29/tatar/releases/latest) sayfasından `.dmg` dosyasını indirin, açın ve `Tatar.app`'i **Uygulamalar** klasörüne sürükleyin.

Uygulama Apple tarafından imzalanmadığı için DMG ile kurulduğunda macOS ilk açılışta **"Tatar" Açılmadı** uyarısı verir:

1. Uyarıda **Bitti**'ye basın (**Çöp Sepeti'ne Taşı**'ya basmayın).
2. **Sistem Ayarları → Gizlilik ve Güvenlik** bölümüne gidin, en altta çıkan **"Yine de Aç"** düğmesine basın ve parolanızı girin.
3. Tatar'ı yeniden açın ve çıkan pencerede **Yine de Aç**'ı seçin.

**İzinler.**

- **Yerel ağ erişimi:** Uygulama ilk kez açıldığında izin isterse **İzin ver**'i seçin. Yoksa telefon bulunamaz.
- **Anahtar zinciri (Keychain):** Güvenli bağlantı için kullanılan özel anahtar macOS anahtar zincirinde saklanır. Parola istendiğinde Mac'inizin giriş parolasını girin ve **Her Zaman İzin Ver** seçeneğini seçin. İmzasız sürümler her güncellemeden sonra bu soruyu yeniden sorar.
- **Müzik ve Spotify (macOS):** Medya kontrolü ilk kullanıldığında macOS "Tatar, Müzik'i (veya Spotify'ı) denetlemek istiyor" diye sorar. **İzin ver**'i seçin; bu izin yalnızca bir kez sorulur.
- **Telefonda bildirim erişimi:** Bildirim yansıtma ve medya kontrolü için telefondaki KDE Connect uygulamasına bildirim erişimi izni verin (Ayarlar → Uygulamalar → KDE Connect → Bildirimler).

### Windows (Windows 10 ve 11, x64)

1. [Sürümler](https://github.com/Augustus29/tatar/releases/latest) sayfasından `*-setup.exe` (veya `.msi`) dosyasını indirin.
2. İndirdiğiniz dosyayı çalıştırın.
3. **Windows SmartScreen** uyarısı çıkarsa **Ek bilgi**'ye, sonra **Yine de çalıştır**'a tıklayın. Uygulama imzasız olduğu için bu uyarı normaldir.
4. Güvenlik duvarı izin penceresi çıkarsa **Özel ağlar** için izin verin. Yalnızca evdeki veya işteki ağ için izin vermek yeterlidir.

## Eşleştirme

1. Telefon ve bilgisayar **aynı Wi-Fi ağında** olmalı.
2. Telefonunuzda **KDE Connect** uygulamasını açın. Listede **Tatar** ile birlikte bilgisayarınızın adı görünmelidir.
3. Bilgisayarın adına dokunun ve **Eşleştir**'e basın.
4. Telefonda ve bilgisayarda aynı **8 karakterlik kod** görünür. İki ekrandaki kodun birebir aynı olduğunu kontrol edin.
5. Her iki tarafta da **Kabul et** deyin.

Bilgisayar listede görünmüyorsa Tatar'ın **Sorun giderme** ekranında **IP adresiyle elle ekle** seçeneğini kullanın ve telefonun IP adresini girin.

**Telefon çalmıyorsa.** Android'de "Aygıtı bul" için telefondaki KDE Connect'in izinleri gerekir: Ayarlar → Uygulamalar → KDE Connect → **Bildirimler** (ve varsa **tam ekran bildirimleri**) açık olmalı; KDE Connect'te bilgisayarınıza dokunup **Eklenti ayarları** → **Telefonumu çaldır** açık ve izinli olmalı.

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

Tatar, [GNU Genel Kamu Lisansı sürüm 3 (GPL-3.0)](LICENSE) ile dağıtılan özgür ve açık kaynak bir yazılımdır. Kodu kullanabilir, değiştirebilir ve paylaşabilirsiniz; değiştirilmiş sürümleri de aynı lisansla, kaynak koduyla birlikte paylaşmanız gerekir.
