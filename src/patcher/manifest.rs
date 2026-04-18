//! Hard-coded list of patch files, ported verbatim from
//! `SeventhUmbral/launcher/PatcherWindow.cpp`. Each entry is the S3 object
//! key, the expected byte size, and the CRC32 of the file body.

pub const PATCH_URL_BASE: &str = "http://ffxivpatches.s3.amazonaws.com/";

#[derive(Debug, Clone, Copy)]
pub struct PatchEntry {
    pub path: &'static str,
    pub size: u64,
    pub crc32: u32,
}

pub const PATCH_MANIFEST: &[PatchEntry] = &[
    PatchEntry { path: "2d2a390f/patch/D2010.09.18.0000.patch", size: 0x0055_0467, crc32: 0x47DDE5ED },

    PatchEntry { path: "48eca647/patch/D2010.09.19.0000.patch", size: 444_398_866, crc32: 0xD55C7ACD },
    PatchEntry { path: "48eca647/patch/D2010.09.23.0000.patch", size: 6_907_277,   crc32: 0xCA135D55 },
    PatchEntry { path: "48eca647/patch/D2010.09.28.0000.patch", size: 18_803_280,  crc32: 0xB19B32FE },

    PatchEntry { path: "48eca647/patch/D2010.10.07.0001.patch", size: 19_226_330,  crc32: 0xD6118CEE },
    PatchEntry { path: "48eca647/patch/D2010.10.14.0000.patch", size: 19_464_329,  crc32: 0x34BF6A99 },
    PatchEntry { path: "48eca647/patch/D2010.10.22.0000.patch", size: 19_778_252,  crc32: 0x2543DB5C },
    PatchEntry { path: "48eca647/patch/D2010.10.26.0000.patch", size: 19_778_391,  crc32: 0x20F94876 },

    PatchEntry { path: "48eca647/patch/D2010.11.25.0002.patch", size: 250_718_651, crc32: 0x5FBB5B24 },
    PatchEntry { path: "48eca647/patch/D2010.11.30.0000.patch", size: 6_921_623,   crc32: 0xA5479111 },

    PatchEntry { path: "48eca647/patch/D2010.12.06.0000.patch", size: 7_158_904,   crc32: 0xCAD6BC31 },
    PatchEntry { path: "48eca647/patch/D2010.12.13.0000.patch", size: 263_311_481, crc32: 0xE51EFC06 },
    PatchEntry { path: "48eca647/patch/D2010.12.21.0000.patch", size: 7_521_358,   crc32: 0x93EE1510 },

    PatchEntry { path: "48eca647/patch/D2011.01.18.0000.patch", size: 9_954_265,   crc32: 0x059E8900 },

    PatchEntry { path: "48eca647/patch/D2011.02.01.0000.patch", size: 11_632_816,  crc32: 0x9EE60B39 },
    PatchEntry { path: "48eca647/patch/D2011.02.10.0000.patch", size: 11_714_096,  crc32: 0x0ADE7243 },

    PatchEntry { path: "48eca647/patch/D2011.03.01.0000.patch", size: 77_464_101,  crc32: 0x7818B5BF },
    PatchEntry { path: "48eca647/patch/D2011.03.24.0000.patch", size: 108_923_937, crc32: 0xF21852AD },
    PatchEntry { path: "48eca647/patch/D2011.03.30.0000.patch", size: 109_010_880, crc32: 0x84CB2682 },

    PatchEntry { path: "48eca647/patch/D2011.04.13.0000.patch", size: 341_603_850, crc32: 0xFF6C3DB0 },
    PatchEntry { path: "48eca647/patch/D2011.04.21.0000.patch", size: 343_579_198, crc32: 0x57F4041C },

    PatchEntry { path: "48eca647/patch/D2011.05.19.0000.patch", size: 344_239_925, crc32: 0xB16FF18C },

    PatchEntry { path: "48eca647/patch/D2011.06.10.0000.patch", size: 344_334_860, crc32: 0xB1CAA88B },

    PatchEntry { path: "48eca647/patch/D2011.07.20.0000.patch", size: 584_926_805, crc32: 0x2EA149A9 },
    PatchEntry { path: "48eca647/patch/D2011.07.26.0000.patch", size: 7_649_141,   crc32: 0x5670BA07 },

    PatchEntry { path: "48eca647/patch/D2011.08.05.0000.patch", size: 152_064_532, crc32: 0x0D9E9FD8 },
    PatchEntry { path: "48eca647/patch/D2011.08.09.0000.patch", size: 8_573_687,   crc32: 0x9B54551A },
    PatchEntry { path: "48eca647/patch/D2011.08.16.0000.patch", size: 6_118_907,   crc32: 0x75231C57 },

    PatchEntry { path: "48eca647/patch/D2011.10.04.0000.patch", size: 677_633_296, crc32: 0x95C15318 },
    PatchEntry { path: "48eca647/patch/D2011.10.12.0001.patch", size: 28_941_655,  crc32: 0xB37993E3 },
    PatchEntry { path: "48eca647/patch/D2011.10.27.0000.patch", size: 29_179_764,  crc32: 0x977480DC },

    PatchEntry { path: "48eca647/patch/D2011.12.14.0000.patch", size: 374_617_428, crc32: 0xC6FE8FED },
    PatchEntry { path: "48eca647/patch/D2011.12.23.0000.patch", size: 22_363_713,  crc32: 0x93137C93 },

    PatchEntry { path: "48eca647/patch/D2012.01.18.0000.patch", size: 48_998_794,  crc32: 0x9E55EC7E },
    PatchEntry { path: "48eca647/patch/D2012.01.24.0000.patch", size: 49_126_606,  crc32: 0x3008D942 },
    PatchEntry { path: "48eca647/patch/D2012.01.31.0000.patch", size: 49_536_396,  crc32: 0x60FDBD0B },

    PatchEntry { path: "48eca647/patch/D2012.03.07.0000.patch", size: 320_630_782, crc32: 0x885AD768 },
    PatchEntry { path: "48eca647/patch/D2012.03.09.0000.patch", size: 8_312_819,   crc32: 0xC0040D8C },
    PatchEntry { path: "48eca647/patch/D2012.03.22.0000.patch", size: 22_027_738,  crc32: 0xEABC501B },
    PatchEntry { path: "48eca647/patch/D2012.03.29.0000.patch", size: 8_322_920,   crc32: 0x63811C35 },

    PatchEntry { path: "48eca647/patch/D2012.04.04.0000.patch", size: 8_678_570,   crc32: 0xF6E43EEC },
    PatchEntry { path: "48eca647/patch/D2012.04.23.0001.patch", size: 289_511_791, crc32: 0x6C3C0201 },

    PatchEntry { path: "48eca647/patch/D2012.05.08.0000.patch", size: 27_266_546,  crc32: 0xB6AABF18 },
    PatchEntry { path: "48eca647/patch/D2012.05.15.0000.patch", size: 27_416_023,  crc32: 0x2D428126 },
    PatchEntry { path: "48eca647/patch/D2012.05.22.0000.patch", size: 27_742_726,  crc32: 0x9163549D },

    PatchEntry { path: "48eca647/patch/D2012.06.06.0000.patch", size: 129_984_024, crc32: 0x21DF7238 },
    PatchEntry { path: "48eca647/patch/D2012.06.19.0000.patch", size: 133_434_217, crc32: 0x8280988A },
    PatchEntry { path: "48eca647/patch/D2012.06.26.0000.patch", size: 133_581_048, crc32: 0x4CF33FC8 },

    PatchEntry { path: "48eca647/patch/D2012.07.21.0000.patch", size: 253_224_781, crc32: 0xA8A42A32 },

    PatchEntry { path: "48eca647/patch/D2012.08.10.0000.patch", size: 42_851_112,  crc32: 0xD8ED4CE3 },

    PatchEntry { path: "48eca647/patch/D2012.09.06.0000.patch", size: 20_566_711,  crc32: 0x4235DF72 },
    PatchEntry { path: "48eca647/patch/D2012.09.19.0001.patch", size: 20_874_726,  crc32: 0x8A775526 },
];

pub fn total_bytes() -> u64 {
    PATCH_MANIFEST.iter().map(|e| e.size).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_not_empty() {
        assert!(!PATCH_MANIFEST.is_empty());
    }

    #[test]
    fn total_size_roughly_matches_historical_download() {
        // PatchProcess.cpp advertises "5.89GB" in the UI, but that's a
        // hand-written string from before the manifest grew; the actual sum
        // of entries comes out at ~6.3 GB.
        let total = total_bytes();
        assert!(total > 6_000_000_000, "got {total}");
        assert!(total < 7_000_000_000, "got {total}");
    }
}
