use std::time::Instant;

mod common;
use common::TestRtc;
use common::init_crypto_default;
use common::init_log;
use common::negotiate;
use str0m::Rtc;
use str0m::change::{SdpAnswer, SdpOffer, SdpPendingOffer};
use str0m::media::{Direction, MediaKind, Mid};
use tracing::info_span;

const SEED: u64 = 1;

fn seed_next_mid() -> Mid {
    fastrand::seed(SEED);
    let mid = Mid::new();
    fastrand::seed(SEED);
    mid
}

fn rtc(name: &'static str) -> TestRtc {
    TestRtc::new_with_rtc(
        info_span!("RTC", name),
        Rtc::builder()
            .clear_codecs()
            .enable_opus(true, false)
            .enable_vp8(true)
            .build(Instant::now()),
    )
}

fn offered_mids(offer: &SdpOffer) -> Vec<String> {
    offer
        .to_sdp_string()
        .lines()
        .filter_map(|l| l.strip_prefix("a=mid:"))
        .map(|m| m.trim().to_string())
        .collect()
}

fn assert_unique(mids: &[String]) {
    for (i, mid) in mids.iter().enumerate() {
        assert!(
            !mids[i + 1..].contains(mid),
            "mid {mid} appears on more than one m-line: {mids:?}"
        );
    }
}

fn answer(r: &mut TestRtc, offer: SdpOffer) -> SdpAnswer {
    r.rtc
        .sdp_api()
        .accept_offer(offer)
        .expect("answerer accepts the offer")
}

fn complete(l: &mut TestRtc, r: &mut TestRtc, offer: SdpOffer, pending: SdpPendingOffer) {
    let answer = answer(r, offer);
    l.rtc
        .sdp_api()
        .accept_answer(pending, answer)
        .expect("offerer accepts the answer");
}

#[test]
pub fn media_and_channel_in_one_change_get_distinct_mids() {
    init_log();
    init_crypto_default();

    let mut l = rtc("L");
    let mut r = rtc("R");

    let mut change = l.rtc.sdp_api();

    let expected = seed_next_mid();
    let audio = change.add_media(MediaKind::Audio, Direction::SendRecv, None, None, None);
    assert_eq!(audio, expected, "the seeded RNG controls the next mid");

    seed_next_mid();
    change.add_channel("data".into());

    let (offer, pending) = change.apply().unwrap();
    let mids = offered_mids(&offer);
    assert_eq!(mids.len(), 2);
    assert_eq!(mids[0], audio.to_string());
    assert_unique(&mids);

    complete(&mut l, &mut r, offer, pending);
    assert!(l.rtc.media(audio).is_some());
    assert!(r.rtc.media(audio).is_some());
}

#[test]
pub fn two_media_in_one_change_get_distinct_mids() {
    init_log();
    init_crypto_default();

    let mut l = rtc("L");
    let mut r = rtc("R");

    let mut change = l.rtc.sdp_api();

    let expected = seed_next_mid();
    let audio = change.add_media(MediaKind::Audio, Direction::SendRecv, None, None, None);
    assert_eq!(audio, expected, "the seeded RNG controls the next mid");

    seed_next_mid();
    let video = change.add_media(MediaKind::Video, Direction::SendRecv, None, None, None);
    assert_ne!(audio, video);

    let (offer, pending) = change.apply().unwrap();
    assert_unique(&offered_mids(&offer));

    complete(&mut l, &mut r, offer, pending);
    assert_eq!(l.rtc.media(audio).unwrap().kind(), MediaKind::Audio);
    assert_eq!(l.rtc.media(video).unwrap().kind(), MediaKind::Video);
}

#[test]
pub fn media_added_before_merge_avoids_pending_mids() {
    init_log();
    init_crypto_default();

    let mut l = rtc("L");
    let mut r = rtc("R");

    let mut change = l.rtc.sdp_api();
    let expected = seed_next_mid();
    let audio = change.add_media(MediaKind::Audio, Direction::SendRecv, None, None, None);
    assert_eq!(audio, expected, "the seeded RNG controls the next mid");
    let (_offer, pending) = change.apply().unwrap();

    let mut change = l.rtc.sdp_api();
    seed_next_mid();
    let video = change.add_media(MediaKind::Video, Direction::SendRecv, None, None, None);
    assert_ne!(audio, video);
    change.merge(pending);

    let (offer, pending) = change.apply().unwrap();
    let mids = offered_mids(&offer);
    assert_eq!(mids.len(), 2);
    assert_unique(&mids);

    complete(&mut l, &mut r, offer, pending);
    assert_eq!(l.rtc.media(audio).unwrap().kind(), MediaKind::Audio);
    assert_eq!(l.rtc.media(video).unwrap().kind(), MediaKind::Video);
}

#[test]
pub fn media_added_after_merge_avoids_pending_mids() {
    init_log();
    init_crypto_default();

    let mut l = rtc("L");
    let mut r = rtc("R");

    let mut change = l.rtc.sdp_api();
    let expected = seed_next_mid();
    change.add_channel("data".into());
    let (_offer, pending) = change.apply().unwrap();

    let mut change = l.rtc.sdp_api();
    change.merge(pending);
    seed_next_mid();
    let audio = change.add_media(MediaKind::Audio, Direction::SendRecv, None, None, None);
    assert_ne!(audio, expected);

    let (offer, pending) = change.apply().unwrap();
    let mids = offered_mids(&offer);
    assert_eq!(mids.len(), 2);
    assert_eq!(mids[0], expected.to_string());
    assert_unique(&mids);

    complete(&mut l, &mut r, offer, pending);
    assert!(l.rtc.media(audio).is_some());
}

#[test]
pub fn media_added_after_channel_is_negotiated_avoids_the_channel_mid() {
    init_log();
    init_crypto_default();

    let mut l = rtc("L");
    let mut r = rtc("R");

    let channel_mid = seed_next_mid();
    negotiate(&mut l, &mut r, |change| {
        change.add_channel("data".into());
    });

    let mut change = l.rtc.sdp_api();
    seed_next_mid();
    let audio = change.add_media(MediaKind::Audio, Direction::SendRecv, None, None, None);
    assert_ne!(audio, channel_mid);

    let (offer, pending) = change.apply().unwrap();
    let mids = offered_mids(&offer);
    assert_eq!(mids, [channel_mid.to_string(), audio.to_string()]);

    complete(&mut l, &mut r, offer, pending);
    assert!(l.rtc.media(audio).is_some());
    assert!(r.rtc.media(audio).is_some());
}

#[test]
pub fn media_added_after_media_is_negotiated_avoids_its_mid() {
    init_log();
    init_crypto_default();

    let mut l = rtc("L");
    let mut r = rtc("R");

    let audio = seed_next_mid();
    negotiate(&mut l, &mut r, |change| {
        change.add_media(MediaKind::Audio, Direction::SendRecv, None, None, None);
    });
    assert!(l.rtc.media(audio).is_some());

    let mut change = l.rtc.sdp_api();
    seed_next_mid();
    let video = change.add_media(MediaKind::Video, Direction::SendRecv, None, None, None);
    assert_ne!(audio, video);

    let (offer, pending) = change.apply().unwrap();
    assert_unique(&offered_mids(&offer));
    complete(&mut l, &mut r, offer, pending);
}
