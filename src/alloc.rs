//! Automatic device allocation shared by Structured Text locals/temporaries and the
//! project's global symbols. Delta editor calls the range setting "Device Resource Allocation";
//! when the range is too small the compile fails ("devices are insufficient").
use crate::plc::{Area, Device};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, str::FromStr};

/// Inclusive ranges the allocator may hand out: `D` words and `M` bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alloc {
    pub d: (u16, u16),
    pub m: (u16, u16),
}
impl Default for Alloc {
    fn default() -> Self {
        Self {
            d: (4000, 4999),
            m: (800, 999),
        }
    }
}
impl Alloc {
    /// General-purpose areas only: the special registers D1000..D1999 / M1000..M1999 are not
    /// handed out, and the ranges must stay inside the addressable memory.
    pub fn validate(&self) -> Result<(), String> {
        let check = |name: &str, (lo, hi): (u16, u16), max: u16| {
            if lo > hi {
                return Err(format!("{name} aralığının başı sonundan büyük olamaz"));
            }
            if hi > max {
                return Err(format!("{name} aralığı {name}{max} sınırını aşıyor"));
            }
            if lo <= 1999 && hi >= 1000 {
                return Err(format!(
                    "{name} aralığı özel alanla ({name}1000..{name}1999) çakışıyor"
                ));
            }
            Ok(())
        };
        check("D", self.d, 9999)?;
        check("M", self.m, 4095)
    }
}

/// Latched (power-fail retentive) general-purpose areas of the ES2/EX2/SS2/SA2/SX2/SE family
/// (manual "Device Range": latched D408~D599, D2000~D3919 and M512~M767, M2048~M4095). The
/// latched area is fixed in hardware and cannot be changed. D/M1000..1999 are special areas.
pub const LATCHED_D: [(u16, u16); 2] = [(408, 599), (2000, 3919)];
pub const LATCHED_M: [(u16, u16); 2] = [(512, 767), (2048, 4095)];

/// Inclusive ranges the allocator hands out to `VAR_RETAIN` variables. They must lie inside one
/// latched segment so the values survive power-off. Absent in saved projects: loads as default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Retain {
    pub d: (u16, u16),
    pub m: (u16, u16),
}
impl Default for Retain {
    fn default() -> Self {
        Self {
            d: (2000, 2999),
            m: (2048, 2303),
        }
    }
}
impl Retain {
    /// Is `index..index+width` entirely inside one latched `D` segment?
    pub fn word_is_latched(index: u16, width: usize) -> bool {
        let end = usize::from(index) + width.max(1) - 1;
        LATCHED_D
            .iter()
            .any(|&(lo, hi)| index >= lo && end <= usize::from(hi))
    }
    pub fn bit_is_latched(index: u16) -> bool {
        LATCHED_M
            .iter()
            .any(|&(lo, hi)| (lo..=hi).contains(&index))
    }
    /// Each range must be ordered, inside a single latched segment (hence clear of the special
    /// areas) and must not overlap the ordinary allocation ranges in `alloc`.
    pub fn validate(&self, alloc: &Alloc) -> Result<(), String> {
        let check = |name: &str,
                     (lo, hi): (u16, u16),
                     segments: &[(u16, u16)],
                     other: (u16, u16)| {
            if lo > hi {
                return Err(format!(
                    "Kalıcı {name} aralığının başı sonundan büyük olamaz"
                ));
            }
            if lo <= 1999 && hi >= 1000 {
                return Err(format!(
                    "Kalıcı {name} aralığı özel alanla ({name}1000..{name}1999) çakışıyor"
                ));
            }
            if !segments.iter().any(|&(a, b)| lo >= a && hi <= b) {
                let list: Vec<String> = segments
                    .iter()
                    .map(|(a, b)| format!("{name}{a}..{name}{b}"))
                    .collect();
                return Err(format!(
                    "Kalıcı {name} aralığı {name}{lo}..{name}{hi} latched alanın ({}) içinde olmalı",
                    list.join(", ")
                ));
            }
            if lo <= other.1 && other.0 <= hi {
                return Err(format!(
                    "Kalıcı {name} aralığı normal otomatik tahsis aralığı {name}{}..{name}{} ile çakışıyor",
                    other.0, other.1
                ));
            }
            Ok(())
        };
        check("D", self.d, &LATCHED_D, alloc.d)?;
        check("M", self.m, &LATCHED_M, alloc.m)
    }
}

/// Hands out free `D` words and `M` bits inside an [`Alloc`], skipping what is taken.
pub struct Allocator {
    alloc: Alloc,
    retain: Retain,
    words: BTreeSet<u16>,
    bits: BTreeSet<u16>,
}
impl Allocator {
    pub fn new(alloc: Alloc) -> Self {
        Self::with_retain(alloc, Retain::default())
    }
    pub fn with_retain(alloc: Alloc, retain: Retain) -> Self {
        Self {
            alloc,
            retain,
            words: BTreeSet::new(),
            bits: BTreeSet::new(),
        }
    }
    /// Mark a device (and, for words, `width` consecutive words) as in use.
    pub fn reserve(&mut self, device: Device, width: usize) {
        match device.area {
            Area::D => self
                .words
                .extend((0..width).map(|i| device.index.saturating_add(i as u16))),
            Area::M => {
                self.bits.insert(device.index);
            }
            _ => {}
        }
    }
    /// Reserve every device spelled in program text. A `D` word is reserved together with its
    /// neighbour so a double-word use of it cannot be overlapped.
    pub fn reserve_text(&mut self, source: &str) {
        for line in source.lines() {
            let code = line.split(';').next().unwrap_or("");
            for token in code.split_whitespace().skip(1) {
                if let Ok(device) = Device::from_str(token) {
                    self.reserve(device, 2);
                }
            }
        }
    }
    pub fn take_bit(&mut self) -> Result<Device, String> {
        self.take_bit_in(self.alloc.m, "otomatik tahsis")
    }
    pub fn take_words(&mut self, width: usize) -> Result<Device, String> {
        self.take_words_in(self.alloc.d, width, "otomatik tahsis")
    }
    /// Like [`Self::take_bit`] but from the latched `VAR_RETAIN` range.
    pub fn take_retain_bit(&mut self) -> Result<Device, String> {
        self.take_bit_in(self.retain.m, "kalıcı (RETAIN) tahsis")
    }
    /// Like [`Self::take_words`] but from the latched `VAR_RETAIN` range.
    pub fn take_retain_words(&mut self, width: usize) -> Result<Device, String> {
        self.take_words_in(self.retain.d, width, "kalıcı (RETAIN) tahsis")
    }
    fn take_bit_in(&mut self, (lo, hi): (u16, u16), what: &str) -> Result<Device, String> {
        let index = (lo..=hi)
            .find(|n| !self.bits.contains(n))
            .ok_or_else(|| format!("M{lo}..M{hi} {what} aralığı dolu"))?;
        self.bits.insert(index);
        Ok(Device {
            area: Area::M,
            index,
        })
    }
    fn take_words_in(
        &mut self,
        (lo, hi): (u16, u16),
        width: usize,
        what: &str,
    ) -> Result<Device, String> {
        let index = (lo..=hi)
            .find(|n| {
                usize::from(*n) + width - 1 <= usize::from(hi)
                    && (0..width).all(|i| !self.words.contains(&(*n + i as u16)))
            })
            .ok_or_else(|| format!("D{lo}..D{hi} {what} aralığı dolu"))?;
        self.words.extend((0..width).map(|i| index + i as u16));
        Ok(Device {
            area: Area::D,
            index,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ranges_are_the_historic_st_pool() {
        let a = Alloc::default();
        assert_eq!((a.d, a.m), ((4000, 4999), (800, 999)));
        a.validate().unwrap();
    }
    #[test]
    fn ranges_must_avoid_special_areas_and_stay_in_memory() {
        let bad = |d, m| Alloc { d, m }.validate().is_err();
        assert!(bad((5000, 4000), (800, 999)), "start after end");
        assert!(bad((4000, 4999), (900, 1100)), "M1000.. is special");
        assert!(bad((500, 1500), (800, 999)), "D1000.. is special");
        assert!(bad((4000, 12000), (800, 999)), "beyond D9999");
        assert!(!bad((2000, 3999), (2000, 4095)));
    }
    #[test]
    fn allocator_skips_reserved_and_double_words() {
        let mut a = Allocator::new(Alloc {
            d: (10, 14),
            m: (0, 2),
        });
        a.reserve_text("LD X0\nMOV K1 D11\nSET M1\nEND\n");
        // D11 is reserved with D12, M1 alone.
        assert_eq!(a.take_words(1).unwrap().index, 10);
        assert_eq!(a.take_words(2).unwrap().index, 13, "needs two free neighbours");
        assert!(a.take_words(1).is_err(), "range exhausted");
        assert_eq!(a.take_bit().unwrap().index, 0);
        assert_eq!(a.take_bit().unwrap().index, 2);
        assert!(a.take_bit().unwrap_err().contains("dolu"));
    }
    fn dev(area: Area, index: u16) -> Device {
        Device { area, index }
    }
    #[test]
    fn retain_default_is_latched_and_clear_of_the_normal_pool() {
        let r = Retain::default();
        r.validate(&Alloc::default()).unwrap();
        assert!(Retain::word_is_latched(r.d.0, 1) && Retain::word_is_latched(r.d.1, 1));
        assert!(Retain::bit_is_latched(r.m.0) && Retain::bit_is_latched(r.m.1));
        assert!(!Retain::word_is_latched(4000, 1), "D4000 is not latched");
        assert!(!Retain::word_is_latched(3919, 2), "pair would leave D3919");
        assert!(!Retain::bit_is_latched(800));
    }
    #[test]
    fn retain_validate_rejects_special_general_and_overlapping_ranges() {
        let alloc = Alloc::default();
        let bad = |d, m| Retain { d, m }.validate(&alloc).is_err();
        assert!(bad((2999, 2000), (2048, 2303)), "start after end");
        assert!(bad((1500, 2500), (2048, 2303)), "D1000..D1999 is special");
        assert!(bad((2000, 2999), (1000, 1100)), "M1000.. is special");
        assert!(bad((600, 700), (2048, 2303)), "D600.. is not latched");
        assert!(bad((300, 450), (2048, 2303)), "straddles latched edge");
        assert!(bad((2000, 4000), (2048, 2303)), "D3920.. is general");
        assert!(bad((4000, 4100), (2048, 2303)), "not latched");
        assert!(bad((2000, 2999), (800, 900)), "M800 is not latched");
        assert!(bad((2000, 2999), (2000, 2100)), "M2000..2047 is general");
        assert!(!bad((408, 599), (512, 767)));
        let overlapping = Alloc {
            d: (2500, 3500),
            m: (800, 999),
        };
        let err = Retain::default().validate(&overlapping).unwrap_err();
        assert!(err.contains("çakışıyor"), "{err}");
    }
    #[test]
    fn retain_allocation_uses_its_own_range_and_honors_reservations() {
        let mut a = Allocator::with_retain(
            Alloc {
                d: (10, 14),
                m: (0, 2),
            },
            Retain {
                d: (2000, 2003),
                m: (2048, 2049),
            },
        );
        a.reserve(dev(Area::D, 2000), 1);
        assert_eq!(a.take_retain_words(2).unwrap().index, 2001);
        assert_eq!(a.take_retain_words(1).unwrap().index, 2003);
        assert!(a.take_retain_words(1).unwrap_err().contains("D2000..D2003"));
        assert_eq!(a.take_retain_bit().unwrap().index, 2048);
        assert_eq!(a.take_retain_bit().unwrap().index, 2049);
        assert!(a.take_retain_bit().unwrap_err().contains("dolu"));
        assert_eq!(a.take_words(1).unwrap().index, 10, "normal pool is separate");
    }
    #[test]
    fn retain_deserializes_from_missing_or_partial_fields() {
        let r: Retain = serde_json::from_str("{}").unwrap();
        assert_eq!(r, Retain::default());
        let r: Retain = serde_json::from_str("{\"d\":[408,420]}").unwrap();
        assert_eq!((r.d, r.m), ((408, 420), Retain::default().m));
    }
}
