//! `kdeconnect.pair` durum makinesi ve doğrulama kodu (protokol 8). Saf mantık: ağ yok.

use sha2::{Digest, Sha256};

/// Zaman damgası penceresi (saniye): |ts - şimdi| > 1800 ise kabul edilmez.
pub const TIMESTAMP_WINDOW_SECS: i64 = 1800;
/// Eşleştirme isteği zaman aşımı.
pub const PAIR_TIMEOUT_SECS: u64 = 30;

/// Doğrulama kodu: SHA-256(büyük_SPKI || küçük_SPKI || ascii(zaman_damgası))'ın ilk 4 baytı,
/// büyük harfli onaltılık (8 karakter).
pub fn verification_key(spki_a: &[u8], spki_b: &[u8], timestamp: i64) -> String {
    let (hi, lo) = if spki_a < spki_b {
        (spki_b, spki_a)
    } else {
        (spki_a, spki_b)
    };
    let mut h = Sha256::new();
    h.update(hi);
    h.update(lo);
    h.update(timestamp.to_string().as_bytes());
    let digest = h.finalize();
    hex::encode_upper(&digest[..4])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairState {
    NotPaired,
    /// Biz istek gönderdik.
    Requested {
        timestamp: i64,
    },
    /// Karşı taraf istek gönderdi, kullanıcı onayı bekleniyor.
    RequestedByPeer {
        timestamp: i64,
    },
    Paired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairInput {
    UserRequest,
    UserAccept,
    /// Gelen isteği reddet, giden isteği iptal et.
    UserReject,
    UserUnpair,
    /// Karşıdan `pair:true` (zaman damgası varsa istektir, yoksa kabuldür).
    PeerPair {
        timestamp: Option<i64>,
    },
    PeerUnpair,
    Timeout,
    LinkLost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailReason {
    Rejected,
    Timeout,
    Cancelled,
    ClockMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairAction {
    SendRequest {
        timestamp: i64,
    },
    SendAccept,
    SendReject,
    /// Kullanıcıya doğrulama koduyla göster.
    NotifyUser {
        timestamp: i64,
    },
    StartTimer,
    CancelTimer,
    /// Sertifikayı sabitle ve cihazı güvenilir olarak kaydet.
    MarkPaired,
    /// Güveni sil.
    Unpaired,
    Failed(FailReason),
}

fn timestamp_ok(ts: Option<i64>, now: i64) -> Option<i64> {
    ts.filter(|t| (t - now).abs() <= TIMESTAMP_WINDOW_SECS)
}

/// Yeni gelen istek: geçerli zaman damgası -> RequestedByPeer, değilse reddet.
fn incoming_request(
    ts: Option<i64>,
    now: i64,
    mut acts: Vec<PairAction>,
) -> (PairState, Vec<PairAction>) {
    match timestamp_ok(ts, now) {
        Some(t) => {
            acts.push(PairAction::NotifyUser { timestamp: t });
            acts.push(PairAction::StartTimer);
            (PairState::RequestedByPeer { timestamp: t }, acts)
        }
        None => {
            acts.push(PairAction::SendReject);
            acts.push(PairAction::Failed(FailReason::ClockMismatch));
            (PairState::NotPaired, acts)
        }
    }
}

pub fn step(state: PairState, input: PairInput, now_secs: i64) -> (PairState, Vec<PairAction>) {
    use PairAction as A;
    use PairInput as I;
    use PairState as S;
    match (state, input) {
        (S::NotPaired, I::UserRequest) => (
            S::Requested {
                timestamp: now_secs,
            },
            vec![
                A::SendRequest {
                    timestamp: now_secs,
                },
                A::StartTimer,
            ],
        ),
        // Karşı taraf zaten istedi: kullanıcı "eşleştir"e basınca kabul say.
        (S::RequestedByPeer { .. }, I::UserRequest)
        | (S::RequestedByPeer { .. }, I::UserAccept) => (
            S::Paired,
            vec![A::CancelTimer, A::SendAccept, A::MarkPaired],
        ),
        (S::Requested { .. }, I::UserReject)
        | (S::Requested { .. }, I::UserUnpair)
        | (S::RequestedByPeer { .. }, I::UserReject)
        | (S::RequestedByPeer { .. }, I::UserUnpair) => (
            S::NotPaired,
            vec![
                A::CancelTimer,
                A::SendReject,
                A::Failed(FailReason::Cancelled),
            ],
        ),
        (S::Paired, I::UserUnpair) | (S::Paired, I::UserReject) => {
            (S::NotPaired, vec![A::SendReject, A::Unpaired])
        }

        // Karşı taraf kabul etti.
        (S::Requested { .. }, I::PeerPair { .. }) => {
            (S::Paired, vec![A::CancelTimer, A::MarkPaired])
        }
        (S::NotPaired, I::PeerPair { timestamp }) => incoming_request(timestamp, now_secs, vec![]),
        (S::RequestedByPeer { .. }, I::PeerPair { .. }) => (state, vec![]),
        // Eşleşmişken: zaman damgasız = gereksiz kabul (yok say); damgalı = karşı taraf güveni
        // kaybetmiş, yerelde eşleşmeyi kaldır ve yeni istek olarak işle.
        (S::Paired, I::PeerPair { timestamp: None }) => (state, vec![]),
        (S::Paired, I::PeerPair { timestamp }) => {
            incoming_request(timestamp, now_secs, vec![A::Unpaired])
        }

        (S::Requested { .. }, I::PeerUnpair) | (S::RequestedByPeer { .. }, I::PeerUnpair) => (
            S::NotPaired,
            vec![A::CancelTimer, A::Failed(FailReason::Rejected)],
        ),
        (S::Paired, I::PeerUnpair) => (S::NotPaired, vec![A::Unpaired]),

        (S::Requested { .. }, I::Timeout) | (S::RequestedByPeer { .. }, I::Timeout) => (
            S::NotPaired,
            vec![A::SendReject, A::Failed(FailReason::Timeout)],
        ),
        (S::Requested { .. }, I::LinkLost) | (S::RequestedByPeer { .. }, I::LinkLost) => (
            S::NotPaired,
            vec![A::CancelTimer, A::Failed(FailReason::Cancelled)],
        ),
        // Diğer her şey: değişiklik yok.
        _ => (state, vec![]),
    }
}

/// Bağlantı üzerinde hangi paketlerin işleneceğinin kararı.
#[derive(Debug, PartialEq, Eq)]
pub enum Disposition {
    /// Eşleştirme paketi: durum makinesine gider.
    Pairing,
    /// Eşleşmiş cihazdan gelen normal paket: eklentilere.
    Plugin,
    /// Eşleşmemiş cihazdan gelen eşleştirme dışı paket: düşür ve logla (yalnızca tür).
    DropUnpaired,
    /// Kimlik paketi bağlantı kurulduktan sonra beklenmez.
    DropUnexpected,
}

pub fn classify(paired: bool, packet_type: &str) -> Disposition {
    match packet_type {
        super::packet::TYPE_PAIR => Disposition::Pairing,
        super::packet::TYPE_IDENTITY => Disposition::DropUnexpected,
        _ if paired => Disposition::Plugin,
        _ => Disposition::DropUnpaired,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PairAction as A;
    use PairInput as I;
    use PairState as S;

    const SPKI_A: &str = "3059301306072a8648ce3d020106082a8648ce3d03010703420004aa3882939b98fac7c0ca8184a422d2e68c28085cfd2090e5ba6e656f9680c6b189efa982cc1467bb616f20f254ffa8f05311f0ce33852943c5fa8e2744f97ae7";
    const SPKI_B: &str = "3059301306072a8648ce3d020106082a8648ce3d03010703420004395b9debac3269b3dbffb2ccd07f478bb6fa068916dfb18fac9cee882497e2d6915e1c3423da7004f9a4f25187a5adcc75788baac451e725316dfc7a97c2e02f";

    #[test]
    fn verification_key_test_vector() {
        let a = hex::decode(SPKI_A).unwrap();
        let b = hex::decode(SPKI_B).unwrap();
        assert_eq!(a.len(), 91);
        assert_eq!(verification_key(&a, &b, 1737228658), "54DC916E");
        assert_eq!(verification_key(&b, &a, 1737228658), "54DC916E");
        assert_eq!(verification_key(&a, &b, 2737228658), "8C07153A");
        assert_eq!(verification_key(&b, &a, 2737228658), "8C07153A");
    }

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn outgoing_request_accepted() {
        let (s, a) = step(S::NotPaired, I::UserRequest, NOW);
        assert_eq!(s, S::Requested { timestamp: NOW });
        assert_eq!(a, vec![A::SendRequest { timestamp: NOW }, A::StartTimer]);
        // tekrar basmak yok sayılır
        assert_eq!(step(s, I::UserRequest, NOW).1, vec![]);
        let (s, a) = step(s, I::PeerPair { timestamp: None }, NOW);
        assert_eq!(s, S::Paired);
        assert!(a.contains(&A::MarkPaired));
    }

    #[test]
    fn outgoing_request_rejected_cancelled_timeout() {
        let req = S::Requested { timestamp: NOW };
        let (s, a) = step(req, I::PeerUnpair, NOW);
        assert_eq!(s, S::NotPaired);
        assert!(a.contains(&A::Failed(FailReason::Rejected)));
        let (s, a) = step(req, I::UserReject, NOW);
        assert_eq!(s, S::NotPaired);
        assert!(a.contains(&A::SendReject));
        let (s, a) = step(req, I::Timeout, NOW);
        assert_eq!(s, S::NotPaired);
        assert!(a.contains(&A::SendReject) && a.contains(&A::Failed(FailReason::Timeout)));
        assert_eq!(step(req, I::LinkLost, NOW).0, S::NotPaired);
    }

    #[test]
    fn incoming_request_flow() {
        let (s, a) = step(
            S::NotPaired,
            I::PeerPair {
                timestamp: Some(NOW + 5),
            },
            NOW,
        );
        assert_eq!(s, S::RequestedByPeer { timestamp: NOW + 5 });
        assert_eq!(a, vec![A::NotifyUser { timestamp: NOW + 5 }, A::StartTimer]);
        // yinelenen istek yok sayılır
        assert_eq!(
            step(
                s,
                I::PeerPair {
                    timestamp: Some(NOW)
                },
                NOW
            ),
            (s, vec![])
        );
        let (s2, a) = step(s, I::UserAccept, NOW);
        assert_eq!(s2, S::Paired);
        assert!(a.contains(&A::SendAccept) && a.contains(&A::MarkPaired));
        // kullanıcı reddederse
        let (s3, a) = step(s, I::UserReject, NOW);
        assert_eq!(s3, S::NotPaired);
        assert!(a.contains(&A::SendReject));
        // "eşleştir"e basmak kabul sayılır
        assert_eq!(step(s, I::UserRequest, NOW).0, S::Paired);
        assert_eq!(step(s, I::Timeout, NOW).0, S::NotPaired);
    }

    #[test]
    fn timestamp_window_and_missing() {
        for ts in [None, Some(NOW + 1801), Some(NOW - 1801)] {
            let (s, a) = step(S::NotPaired, I::PeerPair { timestamp: ts }, NOW);
            assert_eq!(s, S::NotPaired);
            assert!(a.contains(&A::SendReject));
            assert!(a.contains(&A::Failed(FailReason::ClockMismatch)));
        }
        assert_eq!(
            step(
                S::NotPaired,
                I::PeerPair {
                    timestamp: Some(NOW - 1800)
                },
                NOW
            )
            .0,
            S::RequestedByPeer {
                timestamp: NOW - 1800
            }
        );
    }

    #[test]
    fn paired_transitions() {
        // gereksiz kabul yok sayılır
        assert_eq!(
            step(S::Paired, I::PeerPair { timestamp: None }, NOW),
            (S::Paired, vec![])
        );
        // damgalı pair:true = karşı taraf güveni kaybetti
        let (s, a) = step(
            S::Paired,
            I::PeerPair {
                timestamp: Some(NOW),
            },
            NOW,
        );
        assert_eq!(s, S::RequestedByPeer { timestamp: NOW });
        assert_eq!(a[0], A::Unpaired);
        assert!(a.contains(&A::NotifyUser { timestamp: NOW }));
        // uzak eşleşmeyi kaldırdı
        assert_eq!(
            step(S::Paired, I::PeerUnpair, NOW),
            (S::NotPaired, vec![A::Unpaired])
        );
        // kullanıcı kaldırır
        let (s, a) = step(S::Paired, I::UserUnpair, NOW);
        assert_eq!(s, S::NotPaired);
        assert_eq!(a, vec![A::SendReject, A::Unpaired]);
        // bağlantı kopması eşleşmeyi bozmaz; paired'da yeniden istek yok sayılır
        assert_eq!(step(S::Paired, I::LinkLost, NOW), (S::Paired, vec![]));
        assert_eq!(step(S::Paired, I::UserRequest, NOW), (S::Paired, vec![]));
    }

    #[test]
    fn not_paired_noise_ignored() {
        for i in [
            I::PeerUnpair,
            I::UserAccept,
            I::UserReject,
            I::UserUnpair,
            I::Timeout,
            I::LinkLost,
        ] {
            assert_eq!(step(S::NotPaired, i, NOW), (S::NotPaired, vec![]));
        }
    }

    #[test]
    fn packet_filtering() {
        assert_eq!(classify(false, "kdeconnect.pair"), Disposition::Pairing);
        assert_eq!(classify(true, "kdeconnect.pair"), Disposition::Pairing);
        assert_eq!(
            classify(false, "kdeconnect.battery"),
            Disposition::DropUnpaired
        );
        assert_eq!(
            classify(false, "kdeconnect.share.request"),
            Disposition::DropUnpaired
        );
        assert_eq!(classify(true, "kdeconnect.battery"), Disposition::Plugin);
        assert_eq!(
            classify(true, "kdeconnect.identity"),
            Disposition::DropUnexpected
        );
        assert_eq!(
            classify(false, "kdeconnect.identity"),
            Disposition::DropUnexpected
        );
    }
}
