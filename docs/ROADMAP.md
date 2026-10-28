# Tatar — Yol haritası (v1 sonrası)

v1 kapsamı CLAUDE.md §1'de. Aşağıdakiler istenmeden başlanmaz.

- Medya kontrolü (telefondaki müziği bilgisayardan yönetmek)
- Bildirim yansıtma (telefon bildirimlerini bilgisayarda görmek)
- Finder "Paylaş" menüsü eklentisi (macOS) / "Gönder" menüsü (Windows)
- Otomatik güncelleme (imzalama anahtarı gerekir)
- Dock / görev çubuğu ikonunda aktarım ilerlemesi
- Arama gelince müziği duraklatma
- Birden fazla cihazla aynı anda çalışma iyileştirmeleri
- Mobil arayüz uyarlaması
- Windows 11 "snap layouts" (büyüt düğmesinin üzerine gelince açılan yerleşim paneli): özel büyüt düğmesinde çalışmıyor, çünkü WebView2 `WM_NCHITTEST` iletisini yutuyor. Düğme alanını yerel bir `HTCAPTION`/`HTMAXBUTTON` bölgesi olarak işaretleyen bir çözüm araştırılacak.

## Bilinen sorunlar (v1 içinde çözülecek)

- **macOS bildirimleri (doğrulama bekliyor):** Paket artık ad-hoc imzalanıyor (`signingIdentity: "-"`), yetkilendirme hatası günlüğe yazılıyor ve hata durumunda `osascript` yedeği kullanılıyor. Gerçek Mac'te izin penceresinin çıktığı doğrulanacak.
