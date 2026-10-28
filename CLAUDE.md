# Tatar — CLAUDE.md

> Bu dosya Claude Code'un bu projede her oturumun başında okuduğu yol haritasıdır.
> Proje sahibi bir mimar; **kodlama altyapısı yok**. Tüm teknik kararları sen uygularsın,
> ona yalnızca sade Türkçe ile ne yaptığını, neyi nasıl test edeceğini ve vermesi gereken kararları anlatırsın.

---

## 1. Proje özeti

**Tatar**, KDE Connect protokolüyle uyumlu, macOS ve Windows için yazılmış hafif bir masaüstü istemcisidir.
Adını Osmanlı'nın menzilden menzile haber taşıyan posta kuryelerinden (Tatar) alır.

- Telefondaki **resmî KDE Connect uygulaması (Android/iOS) hiç değişmeden** Tatar ile eşleşir. Telefon tarafına kod yazmıyoruz.
- Mevcut KDE Connect masaüstü uygulamasının **fork'u değildir**; protokolü konuşan yeni ve bağımsız bir istemcidir.
- Hedef: macOS'un **Liquid Glass** tasarım diline uygun, animasyonları akıcı, menü çubuğunda yaşayan bir uygulama.
- İki platformda **aynı arayüz**. Tek istisna Windows'ta pencere düğmeleri (bkz. §6.4).

### Kapsam (ilk sürüm, v1)
| Özellik | KDE Connect eklentisi | Not |
|---|---|---|
| Pil durumu | `battery` | Telefonun şarjını göster; %15 altı ve tam dolum bildirimi |
| Ping / Aygıtı bul | `ping`, `findmyphone` | Telefonu çaldır |
| Pano senkronizasyonu | `clipboard` | Otomatik / Elle / Kapalı; **pano geçmişi**; gizli içerik filtresi |
| Dosya gönderme (iki yön) | `share` | Sürükle-bırak, ilerleme, gelen dosya bildirimi |
| Arama gelince sesi kapat | `telephony` (gelen) | Sistem sesini kapat/azalt, arama bitince eski haline getir |

Ek v1 özellikleri: genel klavye kısayolları, sorun giderme ekranı, Türkçe + İngilizce arayüz,
"Saydamlığı azalt" desteği, Windows pencere düğmeleri, ilk açılış akışı.

### Kapsam dışı (v1'de YAPMA)
Uzaktan giriş (mousepad), SMS, medya kontrolü (MPRIS), aygıta göz at (SFTP), komut çalıştırma, bildirim yansıtma,
Bluetooth bağlantısı, Finder "Paylaş" eklentisi, otomatik güncelleme, Dock ilerleme göstergesi.
Bunlar sonraki sürümler için `docs/ROADMAP.md`'ye not edilir; istenmeden başlanmaz.

---

## 2. Teknik kararlar (kesinleşti)

- **Çatı:** Tauri 2 (güncel kararlı sürüm).
- **Arayüz:** React + TypeScript + Vite. Animasyonlar için `motion` (eski adı framer-motion) yay (spring) animasyonları. Stil: düz CSS + CSS değişkenleri (tasarım token'ları `docs/design/DESIGN.md`'de).
- **Arka plan:** Rust (tokio). Protokol, ağ, TLS, eklentiler ve platforma özel kod tamamen Rust tarafında.
- **Arayüz ↔ Rust iletişimi:** Tauri komutları (UI → Rust) ve olaylar (Rust → UI). Arayüz hiçbir zaman doğrudan ağa erişmez.
- **Platformlar:** macOS 13+ (Apple Silicon + Intel), Windows 10/11 (x64). Linux hedef değil ama kodu imkânsız kılma.
- **Geliştirme makinesi:** Mac. Windows sürümü **GitHub Actions** ile derlenir; proje sahibi Windows makinesinde kurup test eder.
- **Dil dosyaları:** `src/i18n/tr.json`, `src/i18n/en.json`. Arayüzdeki **hiçbir metin koda gömülmez**. Varsayılan: sistem dili; Ayarlar'dan değiştirilebilir.

Önerilen Rust kütüphaneleri (kullanmadan önce güncel sürümlerini ve bakım durumlarını kontrol et; daha iyisi varsa gerekçesiyle öner):
`tokio`, `tokio-rustls`/`rustls` (özel sertifika doğrulayıcı ile), `rcgen` (kendinden imzalı sertifika), `mdns-sd` (mDNS),
`serde`/`serde_json`, `keyring` (özel anahtar için işletim sistemi anahtar zinciri), `arboard` (pano; gizli içerik için yerel kod gerekir),
Windows için `windows` crate'i, macOS için `objc2` ailesi. Tauri eklentileri: `global-shortcut`, `notification`, `autostart`,
`dialog`, `opener`, `single-instance`, `store` veya eşdeğeri.

### Önerilen klasör yapısı
```
tatar/
  CLAUDE.md
  docs/
    design/            ← tasarım prototipi ve DESIGN.md (kaynak gerçek)
    ROADMAP.md
    PROTOCOL_NOTES.md  ← protokolü doğrularken tuttuğun notlar
  src/                 ← React arayüz
    components/ screens/ i18n/ styles/ lib/
  src-tauri/
    src/
      main.rs
      protocol/        ← paket, kimlik, keşif, bağlantı, TLS, eşleştirme
      plugins/         ← battery, ping, clipboard, share, telephony
      platform/        ← macos/, windows/ (ses, pano, tepsi, izinler, saydamlık ayarı)
      store/           ← güvenilir cihazlar, ayarlar, aktarım ve pano geçmişi
  .github/workflows/   ← macOS ve Windows derleme
```

---

## 3. KDE Connect protokolü

Protokolün resmî ve eksiksiz bir şartnamesi yok. **Kaynak gerçek, referans uygulamaların kodudur.**
Bir ayrıntıdan emin değilsen tahmin etme. Aşağıdaki kaynaklardan doğrula ve bulduğunu `docs/PROTOCOL_NOTES.md`'ye yaz:

- `kdeconnect-kde` (masaüstü, C++): özellikle `core/backends/lan/lanlinkprovider.cpp`, `core/pairinghandler*`, `plugins/*`
- `kdeconnect-android` (Java): telefon tarafının gerçekte ne gönderdiği
- Valent (C) ve GSConnect (JS): alternatif uygulamalar, okunması kolay
- Lisans uyarısı: Bu projeler GPL lisanslı. **Kodlarını kopyalama**; davranışı oku, kendi kodunu yaz.
  (Proje lisansı henüz seçilmedi, bkz. §10.)

### Bilinen temel noktalar (yine de kaynaktan doğrula)
- Güncel sürüm **protokol 8**. Mart 2025 sonrası KDE Connect sürümleri bunu kullanır. Eski sürümlerle uyumluluk v1'de hedef değil.
- **Keşif:** UDP 1716 üzerinden `kdeconnect.identity` yayını + mDNS (`_kdeconnect._udp`). İkisi birden desteklenecek; bazı ağlar yayını engeller.
- **Bağlantı:** TCP, 1714–1764 aralığında (ana kanal 1716). Dosya aktarımı için ayrı yan kanallar (yaklaşık 1739–1764).
- **Paketler:** satır sonu (`\n`) ile ayrılmış JSON; `id`, `type`, `body` (+ dosya için `payloadSize`, `payloadTransferInfo`).
- **TLS:** Her cihazın kendinden imzalı bir sertifikası vardır (CN = deviceId). TCP bağlantısından sonra TLS'e geçilir. KDE Connect'te TLS istemci/sunucu rolleri TCP rollerine göre **ters** kurulur. Hangi tarafın hangi rolü aldığını `lanlinkprovider.cpp`'den doğrula.
- **Protokol 8 kimlik akışı:** TLS'ten sonra kimlik paketi tekrar gönderilir. **İlk ve ikinci paketteki `deviceId` aynı olmak zorunda**, değilse bağlantıyı kes. (Bu kontrol eksikliği CVE-2025-66270 güvenlik açığına yol açtı.)
- **Eşleştirme:** `kdeconnect.pair`. Protokol 8'de iki tarafta da gösterilen doğrulama kodu sertifikalardan (ve zaman damgasından) türetilir. Kodu KDE Connect ile **birebir aynı** algoritmayla üret, yoksa telefonla eşleşmez. Kısa kodlar kaba kuvvet saldırısına açıktır (CVE-2025-32898). Güncel uzunluğu ve biçimi kaynaktan al.
- **Güven:** Eşleşmiş cihazın sertifikası (veya SHA-256 parmak izi) saklanır. Sonraki bağlantılarda **sertifika birebir eşleşmezse cihaza güvenme**. Eşleşmemiş cihaz yalnızca eşleştirme isteği gönderebilir, başka hiçbir paket işlenmez.
- **Yetenekler:** `incomingCapabilities` / `outgoingCapabilities` listelerine yalnızca gerçekten desteklediğimiz paket türlerini yaz. Telefon arayüzü yalnızca bunları gösterir.
- v1'de ihtiyaç duyulan paket aileleri: `kdeconnect.identity`, `kdeconnect.pair`, `kdeconnect.ping`, `kdeconnect.findmyphone.request`, `kdeconnect.battery` (+ istek paketi varsa), `kdeconnect.clipboard`, `kdeconnect.clipboard.connect`, `kdeconnect.share.request` (+ `.update` varsa), `kdeconnect.telephony`. Alan adlarını ve davranışlarını kaynaktan doğrula.

---

## 4. Güvenlik kuralları (pazarlık konusu değil)

1. Özel anahtar işletim sistemi anahtar zincirinde (macOS Keychain / Windows Credential Manager) saklanır. Düz dosyada **asla**.
2. Ağdan gelen her paket güvenilmez veridir: boyut sınırı koy, JSON'u sıkı doğrula, bilinmeyen türleri yok say ve logla.
3. Gelen dosya adlarını temizle: yol karakterlerini (`../`, `/`, `\`) ve gizli dosyaları reddet. Aynı ad varsa üzerine yazma, `ad (1).uzantı` yap. Yalnızca seçili indirme klasörüne yaz.
4. **Gizli pano içeriği asla gönderilmez ve geçmişe yazılmaz.** macOS'ta `org.nspasteboard.ConcealedType` / `TransientType`, Windows'ta `ExcludeClipboardContentFromMonitorProcessing` ve `CanIncludeInClipboardHistory = 0` formatlarını kontrol et.
5. Pano geçmişi yalnızca bellekte veya şifreli olarak tutulur. En fazla 20 öğe. "Temizle" tamamen siler.
6. Eşleştirme isteğini kullanıcı onaylamadan kabul etme. Doğrulama kodunu her zaman göster.
7. Loglarda pano içeriği, dosya içeriği veya anahtar **yer almaz**.

---

## 5. Platforma özel notlar

### macOS
- Menü çubuğu uygulaması: tepsi ikonu "template image" (siyah siluet, sistem renklendirir). Ana pencere kapatılınca uygulama çalışmaya devam eder. Dock ikonu ayara bağlı.
- `Info.plist`: `NSLocalNetworkUsageDescription` (Türkçe/İngilizce açıklama) ve `NSBonjourServices` içinde `_kdeconnect._udp`. Bunlar yoksa macOS cihaz keşfini sessizce engeller.
- Pencere: gerçek trafik ışıkları (sahtelerini çizme). Başlık çubuğu içeriğin üstüne biner (`titleBarStyle: Overlay`, gizli başlık). Kenar çubuğu için yerel vibrancy kullanılabiliyorsa kullan.
- Sesi kapatma: CoreAudio ile varsayılan çıkış aygıtını sustur. Önceki durumu kaydet, arama bitince geri yükle.
- "Saydamlığı azalt" ayarını oku (`accessibilityDisplayShouldReduceTransparency`) ve değişikliğini dinle; arayüze olay olarak gönder.
- İmzasız uygulama ilk açılışta uyarı verir. README'de "sağ tık → Aç" talimatı olacak. Notarization ileride (§10).

### Windows
- Sistem tepsisi ikonu. Popover, tepsi ikonunun hemen üstünde açılır (görev çubuğu konumuna göre).
- Pencere: `decorations: false`. Sağ üstte Windows 11 tarzı küçült/büyüt/kapat düğmeleri (tasarım: `MainWindows.dc.html`). Sürükleme bölgesi ve Windows 11 "snap layouts" desteği için gereken ayarları araştır.
- Arka plan efekti: Mica/Acrylic varsa kullan, yoksa opak yedek.
- Güvenlik duvarı: ilk açılışta Windows izin penceresi çıkacak. Sorun giderme ekranı kuralı kontrol eder ve gerekirse kullanıcıyı yönlendirir.
- Sesi kapatma: `IAudioEndpointVolume`. Önceki durumu geri yükle.
- "Saydamlık efektleri" ayarı: `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\EnableTransparency`.
- Kısayollarda ⌘ yerine Ctrl, ⌥ yerine Alt.

---

## 6. Tasarım

**Kaynak gerçek:** `docs/design/` klasörü.
- `DESIGN.md`: renkler, cam malzeme tarifleri, köşe yarıçapları, kontroller, animasyon eğrileri.
- `prototype/*.dc.html`: her ekranın tıklanabilir prototip kaynağı. Bunlar `{{...}}` şablon delikleri ve `<sc-if>`/`<sc-for>` etiketleri içeren HTML dosyalarıdır. Stil değerlerini, yerleşimi, metinleri ve davranışları **birebir buradan al**. Bu şablon sözdizimini projeye taşıma; React bileşenlerine çevir.
- Ekranların görsel hali, proje sahibinin Claude'daki "Tatar" tasarım kanvasında. Emin olmadığın görsel bir ayrıntı varsa ondan ekran görüntüsü iste.

### 6.1 Ekran → dosya eşlemesi
| Ekran | Prototip |
|---|---|
| Ana pencere (açık / koyu / saydamlığı azalt / Windows) | `Main.dc.html` (props: `dark`, `solid`, `platform`) |
| Menü çubuğu popover'ı + pano geçmişi | `MenuBar.dc.html` |
| Sistem bildirimleri | `Notifications.dc.html` |
| Eşleştirme (giden / gelen / başarılı) | `Pairing.dc.html` |
| Ayarlar (Genel, Cihaz, Kısayollar, Hakkında; özel açılır menüler) | `Settings.dc.html` |
| İlk açılış | `Onboarding.dc.html` |
| Bağlantı sorunlarını gider | `Troubleshoot.dc.html` |
| Durum ekranları (aktarım başarısız, bağlantı koptu, eşleşmeyi kaldır, cihaz yok) | `States.dc.html` |
| İkon, menü çubuğu simgesi, renkler | `Icon.dc.html` |

### 6.2 Kesin tasarım kuralları
- Kenar çubuğu pencerenin içinde **yüzen** bir cam paneldir (8 px içeride, 18 px köşe). Arkasında masaüstü değil, pencerenin opak zemini vardır. Kenar çubuğu ile içerik arasında **en az 14 px boşluk** bırakılır ve gölge her kenarda görünür. (Proje sahibi bunu özellikle düzeltti, bozma.)
- Araç çubuğunun zemini yok. Düğmeler tek bir cam kapsülde gruplanır, içerik altından kayar ve üstte solma efekti olur.
- Cam yalnızca gezinme ve kontrol katmanında kullanılır. İçerik kartları opak kalır (okunabilirlik).
- Tarayıcının yerel `<select>` menüsü **kullanılmaz**. Açılır menüler özel macOS tarzı bileşendir: seçili öğe düğmenin üstüne hizalanır, yanında tik işareti olur (bkz. `Settings.dc.html`).
- Emoji kullanılmaz. İkonlar ince çizgili SVG'dir (prototiptekiler).
- Metin kontrastı WCAG AA (4,5:1). Bütün etkileşimli öğeler klavyeyle kullanılabilir ve ekran okuyucu etiketlidir.
- Animasyonlar `prefers-reduced-motion` açıkken kapanır. Sistemde saydamlık azaltıldığında cam yüzeyler opak olur (`solid` görünümü).

### 6.3 İkon
Atlı kurye silueti (`Icon.dc.html`). macOS uygulama ikonu için katmanlı Liquid Glass ikon (varsayılan / koyu / renklendirilmiş), Windows için `.ico`, menü çubuğu için siyah template PNG (@1x/@2x). Siluet ilk taslaktır; 16 px'te okunabilirliğini gerçek menü çubuğunda test et ve gerekirse sadeleştirmeyi öner.

### 6.4 Windows istisnası
Arayüz macOS ile birebir aynıdır. Yalnızca pencere kontrol düğmeleri Windows 11 tarzında ve sağ üsttedir; araç çubuğu kapsülü onlara çarpmayacak şekilde sola kayar.

---

## 7. Aşamalı geliştirme planı

Her aşama **çalışan bir uygulama** ile biter. Aşama sonunda:
1. Derle ve çalıştır, kendi testlerini (birim testleri dahil) geçir.
2. Proje sahibine Türkçe, teknik olmayan bir "Test et" listesi ver: neye tıklayacak, ne görmeli.
3. Onun onayını almadan sonraki aşamaya geçme.
4. Onaydan sonra anlamlı bir mesajla commit at.

**Aşama 0: Ortam kurulumu (Mac)**
Gerekli araçları kontrol et ve eksikleri kur: Xcode Command Line Tools, Rust (rustup), Node.js LTS. Her kurulum öncesi ne kuracağını tek cümleyle söyle.
Git deposunu başlat, GitHub deposu oluşturmak için proje sahibine yol göster (veya `gh` ile yap).
*Test:* Boş Tauri uygulaması açılıyor.

**Aşama 1: Kabuk ve tasarım sistemi**
Tasarım token'ları, cam bileşenleri, düğme/anahtar/segment/açılır menü bileşenleri, i18n altyapısı, açık/koyu/saydamlığı azalt temaları.
Ana pencere iskeleti (sahte verilerle, prototipteki gibi), menü çubuğu/tepsi ikonu ve popover, pencere kapatılınca arka planda kalma.
*Test:* Menü çubuğunda atlı ikon var; tıklayınca popover açılıyor; ana pencere prototipe benziyor; sistem karanlık moduna geçince uygulama da geçiyor.

**Aşama 2: Protokol çekirdeği (en kritik aşama)**
Kimlik ve sertifika üretimi, anahtar zinciri, UDP + mDNS keşif, TCP + TLS, protokol 8 kimlik akışı (deviceId kontrolü dahil), eşleştirme ve doğrulama kodu, güvenilir cihaz deposu, otomatik yeniden bağlanma, IP ile elle ekleme.
Önce `docs/PROTOCOL_NOTES.md`'yi doldur, sonra kodla. Paket ayrıştırma ve doğrulama kodu için birim testi yaz.
*Test:* Telefondaki KDE Connect'te "Tatar / MacBook" görünüyor. Eşleştir'e basınca iki tarafta aynı kod çıkıyor, onaylanınca bağlanıyor. Wi‑Fi kapatılıp açılınca kendiliğinden yeniden bağlanıyor.

**Aşama 3: Pil + Ping + Aygıtı bul**
*Test:* Pil yüzdesi ve şarj durumu telefonla aynı. "Aygıtı bul" telefonu çaldırıyor. Telefondan ping gelince bildirim çıkıyor.

**Aşama 4: Pano + pano geçmişi + kısayollar**
Otomatik / Elle / Kapalı modları, gizli içerik filtresi, geçmiş (en fazla 20), genel kısayollar (⇧⌘C, ⇧⌘F, ⌥⌘R, ⌥⌘T, ⇧⌘V) ve kısayol değiştirme.
*Test:* Mac'te kopyalanan metin telefonda yapıştırılabiliyor, tersi de çalışıyor. Şifre yöneticisinden kopyalanan şifre telefona gitmiyor ve geçmişte kilitli görünüyor.

**Aşama 5: Dosya gönderme (iki yön)**
Sürükle-bırak (pencere ve popover), dosya seçici, ilerleme, iptal, hata ve tekrar dene, gelen dosya bildirimi ("Finder'da göster" / "Aç"), indirme klasörü ayarı, güvenli dosya adı.
*Test:* 1 GB'lık bir dosya iki yönde de gidiyor. Aktarım yarıda kesilince "Aktarım başarısız" durumu ve "Tekrar dene" çıkıyor.

**Aşama 6: Arama gelince sesi kapat**
*Test:* Telefon çalınca Mac'in sesi kapanıyor, arama bitince geri açılıyor. Önceden zaten kapalıysa açılmıyor.

**Aşama 7: Akışlar ve kalan ekranlar**
İlk açılış, sorun giderme (gerçek kontrollerle), durum ekranları, ayarların tamamı, bildirimler, oturum açılışında başlat.
*Test:* Uygulamayı sıfırdan kurmuş biri gibi ilk açılış akışını baştan sona geç.

**Aşama 8: Windows**
GitHub Actions ile Windows (ve macOS) derlemesi, Windows'a özel kodlar (tepsi, pencere düğmeleri, ses, pano, güvenlik duvarı, saydamlık ayarı), kurulum dosyası (.msi veya .exe).
*Test:* Proje sahibi Actions'tan indirdiği kurulum dosyasını Windows'a kurup Aşama 2–7 testlerini tekrarlıyor.

**Aşama 9: Cilalama ve yayın hazırlığı**
Animasyon ayarları, erişilebilirlik denetimi, performans (boştayken CPU ≈ 0, bellek düşük), README (TR + EN), ekran görüntüleri, lisans, sürüm 0.1.0.

---

## 8. Çalışma kuralları (Claude Code için)

- **Model ve alt ajanlar:** Proje sahibi kodlamanın **Sonnet** ile yapılmasını istiyor. Planlama ve gözden geçirmeyi sen yap; birbirinden bağımsız kod işlerini (ör. ayrı eklentiler, ayrı ekranlar) **Sonnet modelinde alt ajanlara** dağıt. Alt ajanlara bu dosyanın ilgili bölümünü ve dosya sınırlarını açıkça ver. Sonuçlarını birleştirmeden önce gözden geçir.
  Alt ajanlar kendi alt ajanlarını açamaz (iç içe ajan yok). Protokol/güvenlik/arayüz işleri Sonnet'te, basit mekanik işler (CI, README) Haiku'da yapıldı.
- **Uygulamayı denemek:** `npm run app` uygulamayı yeniden paketleyip açar (paket otomatik güncellenmez). İmzasız derlemelerde macOS her yeni derlemeden sonra Anahtar Zinciri iznini yeniden sorar; onaylanana kadar ağ dinlenmez.
- **İletişim:** Proje sahibiyle **Türkçe** ve sade konuş. Jargonu açıkla veya kullanma. Uzun teknik log yapıştırma, ne olduğunu bir iki cümleyle özetle.
- **Kararlar:** Geri alınması zor veya proje sahibini etkileyen kararları (yeni bağımlılık, lisans, imzalama, veri silme, mimari değişikliği) önce sor. Küçük teknik kararları kendin ver ve bir satırla bildir.
- **Kapsam:** §1'deki kapsam dışı özelliklere istenmeden başlama.
- **Tasarım:** Prototipten sapma gerekiyorsa (teknik bir sınır yüzünden) önce sor, nedenini açıkla.
- **Kalite:** Rust'ta `cargo clippy` ve `cargo fmt`, arayüzde ESLint + Prettier + TypeScript strict. Protokol ve dosya adı temizleme için birim testleri zorunlu.
- **Git:** Her onaylı aşamadan sonra commit. Ara noktalarda küçük commit'ler. Proje sahibi istemedikçe `main` dalına zorla push yapma.
- **Hata ayıklama:** Proje sahibi bir sorun bildirdiğinde önce ne gördüğünü ve ne beklediğini sor, sonra logları incele. Uygulamada gizli bir "Tanılama günlüğünü kopyala" seçeneği olsun (hassas veri içermeyen).
- **Bu dosya:** Bir karar değiştiğinde veya yeni bir şey öğrendiğinde bu dosyayı güncelle ve değişikliği bildir.

---

## 9. Sözlük (proje sahibi için)
- **Protokol:** İki cihazın birbiriyle konuşma kuralları.
- **Eşleştirme:** Telefonla bilgisayarın birbirini bir kez tanıyıp güvenmesi.
- **TLS / sertifika:** Bağlantıyı şifreleyen ve cihazın kimliğini kanıtlayan dijital kimlik kartı.
- **Derleme:** Kodu çalışan bir uygulamaya dönüştürmek.
- **GitHub Actions:** Kodu bulutta otomatik derleyen servis. Windows kurulum dosyası buradan gelir.
- **Commit:** Projenin o anki halini kaydetmek. İstenirse geri dönülebilir.

---

## 10. Açık kararlar (proje sahibine sorulacak)
- [ ] **Lisans:** Açık kaynak paylaşılacaksa MIT/Apache-2.0 (serbest) veya GPL-3.0 (KDE ekosistemiyle uyumlu). Aşama 9'dan önce karar verilecek.
- [x] **Uygulama kimliği:** `io.github.augustus29.tatar`, GitHub kullanıcı adı `Augustus29` (depo: gizli, github.com/Augustus29/tatar).
- [x] **Apple Developer üyeliği:** Alınmayacak (proje gönüllü açık kaynak, maddi beklenti yok). Uygulama imzasız dağıtılır; README'deki "sağ tık → Aç" ve Anahtar Zinciri talimatları geçerli.
- [ ] **macOS 27'ye uyarlama:** Liquid Glass, macOS 27'de kenar çubuğunu pencere kenarına yaslayan rötuşlar aldı. Tasarım şu an yüzen kenar çubuğu üzerine kurulu, değişiklik istenirse küçük bir iş.
