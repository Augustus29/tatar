# Tatar — Tasarım sistemi (Liquid Glass)

Kesin değerler `prototype/*.dc.html` dosyalarındadır. Bu belge onların özetidir. Çelişki olursa prototip geçerlidir.

## Renkler

| Token | Açık | Koyu | Kullanım |
|---|---|---|---|
| `--accent` | `#067682` | `#3FC1CC` | Vurgu (İznik turkuazı): seçili öğe, birincil düğme, anahtar açık |
| `--onAccent` | `#FFFFFF` | `#00282C` | Vurgu üstündeki metin |
| `--accentSoft` | `rgba(10,126,140,.12)` | `rgba(63,193,204,.16)` | İkon zemini, sürükleme alanı |
| `--selBg` | `rgba(10,126,140,.18)` | `rgba(63,193,204,.20)` | Kenar çubuğu seçili satır |
| `--content` / pencere zemini | `#FFFFFF` | `#1E1E20` | Opak pencere ve içerik zemini |
| `--card` | `#F5F5F7` | `#2A2A2D` | İçerik kartları (opak) |
| `--text` | `#1D1D1F` | `#F5F5F7` | Ana metin |
| `--text2` | `#5E5E63` | `#A1A1A6` | İkincil metin (AA kontrastı geçer) |
| `--sep` | `rgba(0,0,0,.09)` | `rgba(255,255,255,.09)` | Ayırıcı çizgi |
| `--green` | `#187A32` | `#4CD964` | Bağlı, pil normal, başarı |
| `--red` | `#C2332A` | `#FF6961` | Hata, yıkıcı eylem, pil zayıf |
| `--solidGlass` | `#ECECF0` | `#2C2C30` | Saydamlığı azalt açıkken camın yerine |

Marka: Derin turkuaz `#0B5F69` (ikon zemini), Kurye kırmızısı `#C8442F` (ikon kuşağı), Parşömen `#F4EBDD` (ikon silueti).
Ayar ikonları için sistem renkleri: turuncu `#E07A1F` (pano), mor `#5B5BD6` (ses), `#6E5BD6` (kısayollar).

## Tipografi
`-apple-system, BlinkMacSystemFont, "SF Pro Text", "Inter", system-ui, sans-serif` (Windows'ta Segoe UI Variable devreye girer).
- Pencere başlığı 15/700 · Bölüm başlığı 13/700 · Gövde 13/400–600 · İkincil 11–12/400 · Kahraman sayı (pil) 34/600 tabular-nums · Cihaz adı 22/700.

## Cam malzeme
- **`.glass`** (araç çubuğu kapsülü, ikincil düğmeler): `rgba(255,255,255,.32)` + `backdrop-filter: blur(22px) saturate(190%) brightness(1.04)` + 1 px `rgba(255,255,255,.55)` kenar + iç üst ışık `inset 0 1px 0 rgba(255,255,255,.85)` + dış gölge `0 10px 30px rgba(0,0,0,.14)`. `::before` ile 135° yumuşak parlama (soft-light).
- **`.glass-strong`** (menü, popover, sheet, bildirim, toast): aynısı, zemin alfa .55 (koyu .6).
- **Kenar çubuğu (`.glass.sbar`)**: `rgba(236,236,241,.72)` (koyu `rgba(48,48,54,.72)`), gölge `0 6px 22px rgba(0,0,0,.10)`. Opak pencere zemini üzerinde yüzer.
- Koyu modda cam: `rgba(40,40,46,.38)`, kenar `rgba(255,255,255,.16)`.
- **Saydamlığı azalt**: bütün cam yüzeyler `--solidGlass`, blur yok, parlama yok, 1 px `--sep` kenar.

## Yerleşim ve yarıçaplar
| Öğe | Değer |
|---|---|
| Araç çubuklu pencere köşesi | 26 px |
| Yalnızca başlık çubuklu pencere | 18 px |
| Kenar çubuğu | 8 px içeride, 18 px köşe, içerikle arasında ≥14 px boşluk |
| Araç çubuğu kapsülü | 36 px yükseklik, 18 px köşe, 36×36 düğmeler |
| İçerik kartları / eylem kutuları | 20–22 px |
| Popover, sheet, bildirim | 22 px |
| Menü | 16 px (açılır menü 12 px), öğe 24–26 px yükseklik, 7–8 px köşe |
| Düğmeler | kapsül (999 px), 28–32 px yükseklik |
| Metin alanı | 30 px yükseklik, 10 px köşe |
| Anahtar (switch) | iz 42×24; topuz 22×20, basılıyken 28 px'e esner |
| Dokunma/tıklama hedefi | en az 28 px (araç çubuğu 36 px) |

## Hareket
_Not (Aşama 9): Açık temadaki `--accent`, `--green`, `--red` değerleri, kart ve saydamlığı-azalt zeminlerinde WCAG AA (4,5:1) metin kontrastı için hafifçe koyulaştırıldı._

- **Yay (spring) — basma ve büyüme:** `cubic-bezier(.34,1.56,.64,1)`, .4–.45 s. Basılınca `scale(.95)`.
- **Menü düğmeden doğar:** `scale(.35)` → 1, opaklık 0 → 1, .42 s `cubic-bezier(.34,1.4,.64,1)`, transform-origin = tetikleyen düğme.
- **Açılır (popup) menü:** hızlı ve sade, `scale(.96)` → 1, .18 s ease-out.
- **Sheet:** araç çubuğundan yaylanarak iner, pencere kenarlarından ayrık, 22 px köşe.
- **Kart girişi:** 6 px yukarı kayma + solma, .45 s `cubic-bezier(.22,1,.36,1)`.
- **Aygıtı bul:** cihaz sallanır (wiggle .55 s) ve 2 nabız halkası 1.6 s döngüde yayılır.
- **Aktarım:** ilerleme çubuğu doğrusal, tamamlanınca toast yaylanarak çıkar.
- **Menü çubuğu ikonu:** aktarım sırasında atlı hafifçe dörtnala gider (±1.5 px, .35 s).
- `prefers-reduced-motion: reduce` → tüm animasyonlar kapalı.

## Metin ve ses tonu
Kısa, sıcak ve teknik olmayan dil. "Cihaz" (aygıt değil, "Aygıtı bul" hariç). Hata mesajları ne olduğunu ve ne yapılacağını söyler ("Bağlantı koptu · %62'de durdu" + "Tekrar dene").
Kısayol gösterimi macOS'ta ⇧⌘C, Windows'ta Ctrl+Shift+C.

## Tasarım kanvası
Görsel referans: proje sahibinin Claude hesabındaki "Tatar" tasarım kanvası (özel). Ekran görüntüsü gerekirse proje sahibinden iste.
