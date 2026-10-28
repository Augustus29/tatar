//! macOS bildirimleri: modern `UNUserNotificationCenter` API'si.
//! (Eski NSUserNotificationCenter yolu yeni macOS'ta banner göstermiyor.)

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AllocAnyThread, define_class, msg_send};
use objc2_foundation::{NSBundle, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationSound,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

const BELIRLENMEDI: u8 = 0;
const VERILDI: u8 = 1;
const REDDEDILDI: u8 = 2;
const KULLANILAMAZ: u8 = 3;

static DURUM: AtomicU8 = AtomicU8::new(BELIRLENMEDI);
static SAYAC: AtomicU64 = AtomicU64::new(0);
static UYARILDI: AtomicU8 = AtomicU8::new(0);

define_class!(
    // Ön plandayken de banner göstermek için temsilci.
    #[unsafe(super(NSObject))]
    #[name = "TatarNotificationDelegate"]
    #[ivars = ()]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }
    }
);

impl Delegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

struct Tutucu(#[allow(dead_code)] Retained<Delegate>);
// Temsilci durumsuz bir NSObject'tir; iş parçacıkları arasında güvenle paylaşılır.
unsafe impl Send for Tutucu {}
unsafe impl Sync for Tutucu {}
static TEMSILCI: OnceLock<Tutucu> = OnceLock::new();

/// Uygulama bir .app paketi olarak mı çalışıyor? (Değilse UN API'si çöker.)
fn paketli() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some()
}

fn uyar_bir_kez(neden: &str) {
    if UYARILDI.swap(1, Ordering::Relaxed) == 0 {
        log::warn!("sistem bildirimleri kullanılamıyor ({neden}); eklenti yedeği kullanılacak");
    }
}

/// Başlangıçta bir kez çağrılır: temsilciyi kurar, izin ister, sonucu günlüğe yazar.
pub fn request_authorization() {
    if !paketli() {
        DURUM.store(KULLANILAMAZ, Ordering::Relaxed);
        log::info!("bildirim izni: belirlenmedi (uygulama paketi dışında çalışıyor)");
        return;
    }
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let _ = TEMSILCI.get_or_init(|| {
        let d = Delegate::new();
        center.setDelegate(Some(ProtocolObject::from_ref(&*d)));
        Tutucu(d)
    });
    let block = RcBlock::new(move |granted: objc2::runtime::Bool, err: *mut NSError| {
        if !err.is_null() {
            // Hata metni içerik taşımaz. Alan/kod, imza sorunlarını (ör. UNErrorDomain 1) ayırt eder.
            let e = unsafe { &*err };
            log::warn!(
                "bildirim izni hatası: {} (alan {}, kod {})",
                e.localizedDescription(),
                e.domain(),
                e.code()
            );
            // İzin istemi hiç gösterilemedi (ör. geçersiz imza): sistem bildirimleri kullanılamaz,
            // osascript yedeğine geçilir.
            DURUM.store(KULLANILAMAZ, Ordering::Relaxed);
            return;
        }
        if granted.as_bool() {
            DURUM.store(VERILDI, Ordering::Relaxed);
            log::info!("bildirim izni: verildi");
        } else {
            DURUM.store(REDDEDILDI, Ordering::Relaxed);
            log::info!("bildirim izni: reddedildi");
        }
    });
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
        &block,
    );
    // Önceden verilmiş/reddedilmiş izni hemen yansıt (istem sonucu beklenmeden).
    let settings_block = RcBlock::new(
        move |s: std::ptr::NonNull<objc2_user_notifications::UNNotificationSettings>| {
            let status = unsafe { s.as_ref() }.authorizationStatus();
            // İstem hatası kalıcı karardır; ayar sorgusu bunu ezmesin.
            if DURUM.load(Ordering::Relaxed) == KULLANILAMAZ {
                return;
            }
            let (kod, ad) = if status == UNAuthorizationStatus::NotDetermined {
                (BELIRLENMEDI, "belirlenmedi")
            } else if status == UNAuthorizationStatus::Denied {
                (REDDEDILDI, "reddedildi")
            } else {
                (VERILDI, "verildi")
            };
            DURUM.store(kod, Ordering::Relaxed);
            log::info!("bildirim izni: {ad}");
        },
    );
    center.getNotificationSettingsWithCompletionHandler(&settings_block);
}

/// Bildirimi gösterir. `false` dönerse çağıran yedeğe geçmelidir.
pub fn show(title: &str, body: &str) -> bool {
    if !paketli() {
        uyar_bir_kez("uygulama paketi dışında");
        return false;
    }
    match DURUM.load(Ordering::Relaxed) {
        REDDEDILDI => {
            uyar_bir_kez("izin reddedildi");
            return false;
        }
        KULLANILAMAZ => {
            uyar_bir_kez("yetkilendirme başarısız");
            return false;
        }
        _ => {}
    }
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    content.setSound(Some(&UNNotificationSound::defaultSound()));
    let n = SAYAC.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let id = NSString::from_str(&format!("tatar-{nanos}-{n}"));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content, None);
    let (baslik, govde) = (title.to_owned(), body.to_owned());
    let block = RcBlock::new(move |err: *mut NSError| {
        if !err.is_null() {
            let e = unsafe { &*err };
            log::warn!(
                "bildirim eklenemedi: {} (kod {}); osascript yedeği kullanılıyor",
                e.localizedDescription(),
                e.code()
            );
            osascript(&baslik, &govde);
        }
    });
    center.addNotificationRequest_withCompletionHandler(&request, Some(&block));
    true
}

/// Yedek yol: `osascript` ile bildirim. Metinler betik içine değil, `argv` ile geçirilir
/// (kaçış/enjeksiyon sorunu olmaz). Sonuç günlüğe yazılır.
pub fn osascript(title: &str, body: &str) {
    let r = std::process::Command::new("/usr/bin/osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display notification (item 2 of argv) with title (item 1 of argv)",
            "-e",
            "end run",
            "--",
            title,
            body,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match r {
        Ok(s) if s.success() => log::info!("bildirim osascript ile gösterildi"),
        Ok(s) => log::warn!("osascript bildirimi başarısız: {s}"),
        Err(e) => log::warn!("osascript çalıştırılamadı: {e}"),
    }
}
