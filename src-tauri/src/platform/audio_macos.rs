//! macOS ses denetimi: CoreAudio, varsayılan çıkış aygıtı.
//! Önce ana (element 0) özellikler denenir; aygıtın ana sesi yoksa 1. ve 2. kanal ayrı ayrı okunur/yazılır.
//! Aygıtta sessiz (mute) özelliği yoksa "sessiz" = ses seviyesi 0 sayılır.

use std::ffi::c_void;
use std::mem::size_of;

use super::OutputState;

type AudioObjectID = u32;
type OSStatus = i32;

#[repr(C)]
#[derive(Clone, Copy)]
struct AudioObjectPropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioObjectHasProperty(id: AudioObjectID, addr: *const AudioObjectPropertyAddress) -> u8;
    fn AudioObjectIsPropertySettable(
        id: AudioObjectID,
        addr: *const AudioObjectPropertyAddress,
        out: *mut u8,
    ) -> OSStatus;
    fn AudioObjectGetPropertyData(
        id: AudioObjectID,
        addr: *const AudioObjectPropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> OSStatus;
    fn AudioObjectSetPropertyData(
        id: AudioObjectID,
        addr: *const AudioObjectPropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> OSStatus;
}

const fn fourcc(s: &[u8; 4]) -> u32 {
    ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | s[3] as u32
}

const SYSTEM_OBJECT: AudioObjectID = 1;
const SEL_DEFAULT_OUTPUT: u32 = fourcc(b"dOut");
const SEL_MUTE: u32 = fourcc(b"mute");
const SEL_VOLUME: u32 = fourcc(b"volm");
const SEL_STEREO_CHANNELS: u32 = fourcc(b"dch2");
const SCOPE_GLOBAL: u32 = fourcc(b"glob");
const SCOPE_OUTPUT: u32 = fourcc(b"outp");

fn addr(selector: u32, scope: u32, element: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        selector,
        scope,
        element,
    }
}

fn has(dev: AudioObjectID, a: &AudioObjectPropertyAddress) -> bool {
    // SAFETY: a geçerli bir yerel yapıya işaret ediyor.
    unsafe { AudioObjectHasProperty(dev, a) != 0 }
}

fn settable(dev: AudioObjectID, a: &AudioObjectPropertyAddress) -> bool {
    let mut out: u8 = 0;
    // SAFETY: işaretçiler yerel değişkenlere ait.
    let st = unsafe { AudioObjectIsPropertySettable(dev, a, &mut out) };
    st == 0 && out != 0
}

fn get_u32(dev: AudioObjectID, a: &AudioObjectPropertyAddress) -> Option<u32> {
    let mut v: u32 = 0;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: v, size boyutuyla uyumlu bir tampon.
    let st = unsafe {
        AudioObjectGetPropertyData(
            dev,
            a,
            0,
            std::ptr::null(),
            &mut size,
            &mut v as *mut u32 as *mut c_void,
        )
    };
    (st == 0).then_some(v)
}

fn set_u32(dev: AudioObjectID, a: &AudioObjectPropertyAddress, v: u32) -> bool {
    // SAFETY: v, size boyutuyla uyumlu bir tampon.
    let st = unsafe {
        AudioObjectSetPropertyData(
            dev,
            a,
            0,
            std::ptr::null(),
            size_of::<u32>() as u32,
            &v as *const u32 as *const c_void,
        )
    };
    st == 0
}

fn get_f32(dev: AudioObjectID, a: &AudioObjectPropertyAddress) -> Option<f32> {
    let mut v: f32 = 0.0;
    let mut size = size_of::<f32>() as u32;
    // SAFETY: v, size boyutuyla uyumlu bir tampon.
    let st = unsafe {
        AudioObjectGetPropertyData(
            dev,
            a,
            0,
            std::ptr::null(),
            &mut size,
            &mut v as *mut f32 as *mut c_void,
        )
    };
    (st == 0).then_some(v)
}

fn set_f32(dev: AudioObjectID, a: &AudioObjectPropertyAddress, v: f32) -> bool {
    // SAFETY: v, size boyutuyla uyumlu bir tampon.
    let st = unsafe {
        AudioObjectSetPropertyData(
            dev,
            a,
            0,
            std::ptr::null(),
            size_of::<f32>() as u32,
            &v as *const f32 as *const c_void,
        )
    };
    st == 0
}

fn default_output() -> Option<AudioObjectID> {
    let a = addr(SEL_DEFAULT_OUTPUT, SCOPE_GLOBAL, 0);
    let id = get_u32(SYSTEM_OBJECT, &a)?;
    (id != 0).then_some(id)
}

/// Ses/sessiz özelliklerinin yazılacağı elementler: ana (0) varsa o, yoksa stereo kanalları.
fn elements(dev: AudioObjectID, selector: u32) -> Vec<u32> {
    if has(dev, &addr(selector, SCOPE_OUTPUT, 0)) {
        return vec![0];
    }
    let a = addr(SEL_STEREO_CHANNELS, SCOPE_OUTPUT, 0);
    let mut chans = [1u32, 2u32];
    let mut size = size_of::<[u32; 2]>() as u32;
    // SAFETY: chans, size boyutuyla uyumlu bir tampon.
    let st = unsafe {
        AudioObjectGetPropertyData(
            dev,
            &a,
            0,
            std::ptr::null(),
            &mut size,
            chans.as_mut_ptr() as *mut c_void,
        )
    };
    if st != 0 {
        chans = [1, 2];
    }
    chans
        .into_iter()
        .filter(|c| has(dev, &addr(selector, SCOPE_OUTPUT, *c)))
        .collect()
}

pub fn get_output_state() -> Option<OutputState> {
    let dev = default_output()?;
    let vol_els = elements(dev, SEL_VOLUME);
    let vols: Vec<f32> = vol_els
        .iter()
        .filter_map(|e| get_f32(dev, &addr(SEL_VOLUME, SCOPE_OUTPUT, *e)))
        .collect();
    let volume = if vols.is_empty() {
        None
    } else {
        Some((vols.iter().sum::<f32>() / vols.len() as f32).clamp(0.0, 1.0))
    };
    let mute_els = elements(dev, SEL_MUTE);
    let mutes: Vec<bool> = mute_els
        .iter()
        .filter_map(|e| get_u32(dev, &addr(SEL_MUTE, SCOPE_OUTPUT, *e)))
        .map(|v| v != 0)
        .collect();
    let muted = if mutes.is_empty() {
        // Mute yoksa sessiz = ses 0.
        volume.is_some_and(|v| v <= 0.0)
    } else {
        mutes.iter().all(|m| *m)
    };
    if volume.is_none() && mutes.is_empty() {
        return None; // ör. HDMI: ses denetimi yok
    }
    Some(OutputState {
        muted,
        volume: volume.unwrap_or(1.0),
    })
}

pub fn set_output_state(state: OutputState) -> bool {
    let Some(dev) = default_output() else {
        return false;
    };
    let vol_els = elements(dev, SEL_VOLUME);
    let mute_els: Vec<u32> = elements(dev, SEL_MUTE)
        .into_iter()
        .filter(|e| settable(dev, &addr(SEL_MUTE, SCOPE_OUTPUT, *e)))
        .collect();
    let volume = state.volume.clamp(0.0, 1.0);
    let mut ok = true;
    let mut touched = false;

    if mute_els.is_empty() {
        // Mute özelliği yok: sessizleştirme = ses 0; açma = kayıtlı ses seviyesi.
        let target = if state.muted { 0.0 } else { volume };
        for e in &vol_els {
            touched = true;
            ok &= set_f32(dev, &addr(SEL_VOLUME, SCOPE_OUTPUT, *e), target);
        }
    } else {
        for e in &mute_els {
            touched = true;
            ok &= set_u32(
                dev,
                &addr(SEL_MUTE, SCOPE_OUTPUT, *e),
                u32::from(state.muted),
            );
        }
        for e in &vol_els {
            if settable(dev, &addr(SEL_VOLUME, SCOPE_OUTPUT, *e)) {
                touched = true;
                ok &= set_f32(dev, &addr(SEL_VOLUME, SCOPE_OUTPUT, *e), volume);
            }
        }
    }
    touched && ok
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Salt okunur: gerçek CoreAudio çağrısı çökmeden çalışmalı, değerler aralıkta olmalı.
    #[test]
    fn reads_default_output_without_crashing() {
        if let Some(s) = get_output_state() {
            assert!((0.0..=1.0).contains(&s.volume));
        }
    }
}
