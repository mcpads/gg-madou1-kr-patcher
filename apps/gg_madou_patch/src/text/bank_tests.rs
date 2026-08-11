use super::*;
use std::collections::HashSet;

fn make_test_en_rom() -> Vec<u8> {
    // Table 0 (main $8000): ptrs @$0000 (4 entries)
    //   e0=null, e1→$0200 "AB[FE]", e2→$0210 "CDEF[FE]", e3→$0213 shares "F[FE]"
    // Table 1 (secondary $8100): ptrs @$0100 (4 entries)
    //   e1→$0200 (bank $0300) "GH[FE]"
    // Graphic block @$0400 (16B, NON-text, referenced by NO pointer) — must be protected.
    let mut rom = vec![0u8; 0x4000];

    // Table 0 pointers
    rom[0x02] = 0x00;
    rom[0x03] = 0x02; // e1 → $0200
    rom[0x04] = 0x10;
    rom[0x05] = 0x02; // e2 → $0210
    rom[0x06] = 0x13;
    rom[0x07] = 0x02; // e3 → $0213 (suffix of e2)

    // Table 0 text
    rom[0x200] = 0x41;
    rom[0x201] = 0x42;
    rom[0x202] = 0xFE; // e1 "AB[FE]"
    rom[0x210] = 0x43;
    rom[0x211] = 0x44;
    rom[0x212] = 0x45;
    rom[0x213] = 0x46;
    rom[0x214] = 0xFE; // e2 "CDEF[FE]"; e3 → $0213 "F[FE]"

    // Table 1 pointers
    rom[0x102] = 0x00;
    rom[0x103] = 0x02; // e1 → $0200 (rel $8100) = bank $0300
    rom[0x300] = 0x47;
    rom[0x301] = 0x48;
    rom[0x302] = 0xFE; // "GH[FE]"

    // Graphic block @$0400 (distinctive, non-$FE, not referenced by any pointer)
    for (i, b) in [
        0xAA, 0x01, 0xBB, 0x02, 0xCC, 0x03, 0xDD, 0x04, 0xEE, 0x05, 0x11, 0x06, 0x22, 0x07, 0x33,
        0x08,
    ]
    .iter()
    .enumerate()
    {
        rom[0x400 + i] = *b;
    }
    rom
}

static TEST_MULTI_CONFIG: MultiBankConfig = MultiBankConfig {
    bank_number: 99,
    physical_base: 0,
    reserved_ranges: &[],
    tables: &[
        SubTable {
            name: "main",
            base_logical: 0x8000,
            ptr_bank_offset: 0x0000,
            ptr_count: 4,
        },
        SubTable {
            name: "secondary",
            base_logical: 0x8100,
            ptr_bank_offset: 0x0100,
            ptr_count: 4,
        },
    ],
};

#[test]
fn test_multi_table_no_translations() {
    let en_rom = make_test_en_rom();

    // No translations — entire bank should be identical to EN ROM
    let result = build_bank_with_tables(&TEST_MULTI_CONFIG, &[], &en_rom).unwrap();
    assert_eq!(result, en_rom);
}

#[test]
fn test_multi_table_preserves_secondary() {
    let en_rom = make_test_en_rom();
    let translations = vec![TableTranslation {
        table_idx: 0,
        entries: vec![EncodedEntry {
            id: 1,
            data: vec![0x58, 0x59, 0xFE],
            skip: false,
        }],
    }];
    let result = build_bank_with_tables(&TEST_MULTI_CONFIG, &translations, &en_rom).unwrap();
    let ptr1 = u16::from_le_bytes([result[0x02], result[0x03]]);
    let bank_offset = ptr1 as usize;
    assert_eq!(result[bank_offset], 0x58);
    assert_eq!(result[bank_offset + 1], 0x59);
    assert_eq!(result[bank_offset + 2], 0xFE);
    // Secondary + untranslated e2/e3 preserved
    assert_eq!(result[0x102], en_rom[0x102]);
    assert_eq!(result[0x300], 0x47);
    assert_eq!(result[0x210], 0x43); // e2 "C" preserved
    assert_eq!(result[0x213], 0x46); // e3-shared "F" preserved
}

#[test]
fn test_multi_table_suffix_sharing_preserved() {
    let en_rom = make_test_en_rom();
    let translations = vec![TableTranslation {
        table_idx: 0,
        entries: vec![
            EncodedEntry {
                id: 2,
                data: vec![0x5A, 0x5B, 0xFE],
                skip: false,
            }, // 3B fits in $0210-$0212
            EncodedEntry {
                id: 3,
                data: Vec::new(),
                skip: true,
            },
        ],
    }];
    let result = build_bank_with_tables(&TEST_MULTI_CONFIG, &translations, &en_rom).unwrap();
    // e3 shares suffix — pointer $0213 preserved, "F[FE]" preserved
    let ptr3 = u16::from_le_bytes([result[0x06], result[0x07]]);
    assert_eq!(ptr3, 0x0213);
    assert_eq!(result[0x213], 0x46); // 'F'
    assert_eq!(result[0x214], 0xFE);
}

#[test]
fn test_multi_table_pointer_value_secondary() {
    let en_rom = make_test_en_rom();
    let translations = vec![TableTranslation {
        table_idx: 1,
        entries: vec![EncodedEntry {
            id: 1,
            data: vec![0x70, 0x71, 0xFE],
            skip: false,
        }],
    }];
    let result = build_bank_with_tables(&TEST_MULTI_CONFIG, &translations, &en_rom).unwrap();
    let ptr1 = u16::from_le_bytes([result[0x102], result[0x103]]);
    let bank_offset = ptr1 as usize + 0x100;
    assert_eq!(result[bank_offset], 0x70);
    assert_eq!(result[bank_offset + 1], 0x71);
    assert_eq!(result[bank_offset + 2], 0xFE);
    assert_eq!(result[0x200], 0x41); // main preserved
}

#[test]
fn whitelist_excludes_unreferenced_graphic() {
    let en = make_test_en_rom();
    // Translate main e1 only.
    let translated: HashSet<(usize, u16)> = [(0usize, 1u16)].into_iter().collect();
    let protected = compute_protected_bytes(&TEST_MULTI_CONFIG, &en, &translated);
    let tbytes = compute_translated_text_bytes(&TEST_MULTI_CONFIG, &en, &translated);
    let writable = compute_writable_regions(&tbytes, &protected);
    // Graphic block $0400-$040F must NOT be inside any writable region.
    let in_writable = |off: usize| writable.iter().any(|&(s, e)| off >= s && off < e);
    for off in 0x400..0x410 {
        assert!(
            !in_writable(off),
            "graphic byte ${off:04X} wrongly writable"
        );
    }
}

#[test]
fn whitelist_includes_reclaimed_translated_text() {
    let en = make_test_en_rom();
    let translated: HashSet<(usize, u16)> = [(0usize, 1u16)].into_iter().collect();
    let protected = compute_protected_bytes(&TEST_MULTI_CONFIG, &en, &translated);
    let tbytes = compute_translated_text_bytes(&TEST_MULTI_CONFIG, &en, &translated);
    let writable = compute_writable_regions(&tbytes, &protected);
    let in_writable = |off: usize| writable.iter().any(|&(s, e)| off >= s && off < e);
    // e1's EN text $0200-$0202 is reclaimable → writable.
    for off in 0x200..0x203 {
        assert!(
            in_writable(off),
            "reclaimed byte ${off:04X} should be writable"
        );
    }
    // Untranslated e2/e3 text $0210-$0214 must NOT be writable (protected).
    for off in 0x210..0x215 {
        assert!(
            !in_writable(off),
            "untranslated byte ${off:04X} must be protected"
        );
    }
}

#[test]
fn whitelist_respects_reserved_ranges() {
    // reserved_ranges (BANK5 uses these) must stay protected even inside a
    // translated entry's text.
    static RESV_CONFIG: MultiBankConfig = MultiBankConfig {
        bank_number: 98,
        physical_base: 0,
        reserved_ranges: &[(0x0202, 0x0203)], // byte $0202 (e1's FE terminator)
        tables: &[SubTable {
            name: "main",
            base_logical: 0x8000,
            ptr_bank_offset: 0x0000,
            ptr_count: 4,
        }],
    };
    let en = make_test_en_rom();
    let translated: HashSet<(usize, u16)> = [(0usize, 1u16)].into_iter().collect();
    let protected = compute_protected_bytes(&RESV_CONFIG, &en, &translated);
    let tbytes = compute_translated_text_bytes(&RESV_CONFIG, &en, &translated);
    let writable = compute_writable_regions(&tbytes, &protected);
    let in_writable = |off: usize| writable.iter().any(|&(s, e)| off >= s && off < e);
    assert!(!in_writable(0x202), "reserved byte $0202 must be protected");
    assert!(
        in_writable(0x200),
        "non-reserved e1 text $0200 should be writable"
    );
}

#[test]
fn test_region_allocator_basic() {
    let mut alloc = RegionAllocator::new(vec![(10, 15), (20, 30)]);

    assert_eq!(alloc.alloc(3, 0).unwrap(), 10);
    assert_eq!(alloc.alloc(2, 0).unwrap(), 13);
    // Region 0 full (5 bytes used), should move to region 1
    assert_eq!(alloc.alloc(5, 0).unwrap(), 20);
    assert_eq!(alloc.alloc(5, 0).unwrap(), 25);
    // Should fail (only 0 bytes left)
    assert!(alloc.alloc(1, 0).is_err());
}

#[test]
fn test_region_allocator_overflow() {
    let mut alloc = RegionAllocator::new(vec![(10, 12)]);
    assert_eq!(alloc.alloc(2, 0).unwrap(), 10);
    assert!(alloc.alloc(1, 0).is_err());
}

#[test]
fn test_region_allocator_min_offset() {
    let mut alloc = RegionAllocator::new(vec![(10, 20), (30, 40)]);

    // min_offset=25 should skip region 0 and allocate in region 1
    assert_eq!(alloc.alloc(3, 25).unwrap(), 30);
    // min_offset=0 can use the earlier range left behind by the high-min allocation.
    assert_eq!(alloc.alloc(3, 0).unwrap(), 10);
}

// --- 순수 검증 함수 직접 테스트 (Task 2 신규 코드를 실제로 실행하는 영구 회귀테스트) ---
#[test]
fn verify_rejects_change_outside_whitelist() {
    let en = vec![0u8; 0x20];
    let mut bank = en.clone();
    bank[0x10] = 0xAB; // changed, but not in writable nor ptr
    let writable: HashSet<usize> = HashSet::new();
    let ptr: HashSet<usize> = HashSet::new();
    assert!(verify_bank_within_whitelist(&bank, &en, &writable, &ptr, 3).is_err());
}

#[test]
fn verify_allows_change_inside_writable_or_ptr() {
    let en = vec![0u8; 0x20];
    let mut bank = en.clone();
    bank[0x10] = 0xAB; // in writable
    bank[0x05] = 0xCD; // in ptr
    let writable: HashSet<usize> = [0x10usize].into_iter().collect();
    let ptr: HashSet<usize> = [0x05usize].into_iter().collect();
    assert!(verify_bank_within_whitelist(&bank, &en, &writable, &ptr, 3).is_ok());
}

// --- 통합: 화이트리스트가 그래픽을 지키고 오버플로우는 fail-loud ---
#[test]
fn build_preserves_graphic_block() {
    let en_rom = make_test_en_rom();
    let translations = vec![TableTranslation {
        table_idx: 0,
        entries: vec![EncodedEntry {
            id: 1,
            data: vec![0x58, 0x59, 0xFE],
            skip: false,
        }],
    }];
    let result = build_bank_with_tables(&TEST_MULTI_CONFIG, &translations, &en_rom).unwrap();
    assert_eq!(
        &result[0x400..0x410],
        &en_rom[0x400..0x410],
        "graphic block clobbered"
    );
}

#[test]
fn build_overflows_when_ko_exceeds_reclaim() {
    let en_rom = make_test_en_rom();
    // e1 reclaim is 3B ($0200-$0202); a 10B KO cannot fit and no free space is writable.
    let translations = vec![TableTranslation {
        table_idx: 0,
        entries: vec![EncodedEntry {
            id: 1,
            data: vec![0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0xFE],
            skip: false,
        }],
    }];
    assert!(build_bank_with_tables(&TEST_MULTI_CONFIG, &translations, &en_rom).is_err());
}

// --- Task 9: original_bank_untouched_when_table_relocated (build_bank_with_tables) ---
// Proves design §3.3: excluding a table from `translations` entirely leaves
// its ptr table + text region byte-identical to EN (the "명시 제외" contract
// that lets the relocated table's original slot double as a safe fallback).
#[test]
fn original_bank_untouched_when_table_relocated() {
    let en_rom = make_test_en_rom();
    // table_idx 0 ("main") is entirely absent — simulates it being relocated
    // to a spare bank and excluded from this bank's translation set.
    let translations = vec![TableTranslation {
        table_idx: 1,
        entries: vec![EncodedEntry {
            id: 1,
            data: vec![0x70, 0x71, 0xFE],
            skip: false,
        }],
    }];
    let result = build_bank_with_tables(&TEST_MULTI_CONFIG, &translations, &en_rom).unwrap();
    // main (relocated away) must be byte-identical to EN: ptr table + text.
    assert_eq!(
        &result[0x000..0x008],
        &en_rom[0x000..0x008],
        "main ptr table changed though its table was excluded"
    );
    assert_eq!(
        &result[0x200..0x215],
        &en_rom[0x200..0x215],
        "main text region changed though its table was excluded"
    );
    // secondary (still present in `translations`) was translated normally.
    let ptr1 = u16::from_le_bytes([result[0x102], result[0x103]]);
    let bank_offset = ptr1 as usize + 0x100;
    assert_eq!(result[bank_offset], 0x70);
}

// =====================================================================
// Task 1 (Phase 3b): build_relocated_bank — spare-bank standalone builder.
//
// Fixture: a synthetic 0x8000-byte "ROM" where [0,0x4000) plays the role of
// the *source* bank (reuses make_test_en_rom's main/secondary layout) and
// [0x4000,0x8000) plays the role of a blank spare bank (all $FF, matching
// the real spare banks 35-63 per phase3a_whitelist_result.md §6).
// =====================================================================

fn make_test_relocation_rom() -> Vec<u8> {
    let mut rom = vec![0xFFu8; 0x8000];
    rom[0..0x4000].copy_from_slice(&make_test_en_rom());
    rom
}

/// Like `make_test_relocation_rom`, but with the source's pointer tables
/// (main @$0000, secondary @$0100) all-null — no EN entries are active at
/// all. Tests that need fully deterministic allocation offsets use this
/// instead of `make_test_relocation_rom`, whose shared main-table fixture
/// (ids 1/2/3 all active) pulls in raw-EN-copy entries for any id not
/// explicitly overridden, which is correct product behavior (§2.2 step 6a)
/// but makes hand-computed absolute offsets fragile/misleading for tests
/// that only care about *relative* allocator behavior (region choice,
/// suffix-sharing scope) rather than the raw-copy fallback itself.
fn make_blank_relocation_rom() -> Vec<u8> {
    let mut rom = vec![0xFFu8; 0x8000];
    rom[..0x200].fill(0x00);
    rom
}

/// Dest config: duplicates TEST_MULTI_CONFIG's SubTable values verbatim
/// (§1 invariant) but points at the blank second half of the fixture ROM.
static TEST_DEST_CONFIG: MultiBankConfig = MultiBankConfig {
    bank_number: 199,
    physical_base: 0x4000,
    reserved_ranges: &[],
    tables: &[
        SubTable {
            name: "main",
            base_logical: 0x8000,
            ptr_bank_offset: 0x0000,
            ptr_count: 4,
        },
        SubTable {
            name: "secondary",
            base_logical: 0x8100,
            ptr_bank_offset: 0x0100,
            ptr_count: 4,
        },
    ],
};

#[test]
fn relocated_translated_and_untranslated_ids() {
    let rom = make_test_relocation_rom();
    let source = RelocatedTableSource {
        source_physical_base: 0,
        entries: &[
            EncodedEntry {
                id: 1,
                data: vec![0x80, 0x01, 0xFE],
                skip: false,
            },
            EncodedEntry {
                id: 2,
                data: Vec::new(),
                skip: true,
            },
        ],
    };
    let dest = MultiBankConfig {
        bank_number: 199,
        physical_base: 0x4000,
        reserved_ranges: &[],
        tables: &TEST_DEST_CONFIG.tables[0..1],
    };
    let result = build_relocated_bank(&dest, &[source], &rom).unwrap();

    // id1 (translated): KO bytes written verbatim, no re-encoding.
    let ptr1 = u16::from_le_bytes([result[0x02], result[0x03]]);
    let off1 = ptr1 as usize;
    assert_eq!(&result[off1..off1 + 3], &[0x80, 0x01, 0xFE]);

    // id2 (skip/untranslated): raw EN bytes copied verbatim from the source
    // bank's text ("CDEF[FE]" at source $0210, per make_test_en_rom).
    let expected = entry_text_range(&rom, 0, &dest.tables[0], 2)
        .map(|r| rom[r].to_vec())
        .unwrap();
    assert_eq!(expected, vec![0x43, 0x44, 0x45, 0x46, 0xFE]);
    let ptr2 = u16::from_le_bytes([result[0x04], result[0x05]]);
    let off2 = ptr2 as usize;
    assert_eq!(&result[off2..off2 + expected.len()], expected.as_slice());
}

#[test]
fn relocated_inactive_id_stays_null() {
    let rom = make_test_relocation_rom();
    // id0 has no EN pointer (null in make_test_en_rom) and no translation.
    let source = RelocatedTableSource {
        source_physical_base: 0,
        entries: &[],
    };
    let dest = MultiBankConfig {
        bank_number: 199,
        physical_base: 0x4000,
        reserved_ranges: &[],
        tables: &TEST_DEST_CONFIG.tables[0..1],
    };
    let result = build_relocated_bank(&dest, &[source], &rom).unwrap();
    assert_eq!(result[0x00], 0x00);
    assert_eq!(result[0x01], 0x00);
}

#[test]
fn relocated_preserves_base_logical_and_ptr_math() {
    let rom = make_test_relocation_rom();
    // secondary table only: base_logical=0x8100 != 0x8000.
    let source = RelocatedTableSource {
        source_physical_base: 0,
        entries: &[EncodedEntry {
            id: 1,
            data: vec![0xAA, 0xBB, 0xCC, 0xFE],
            skip: false,
        }],
    };
    let dest = MultiBankConfig {
        bank_number: 199,
        physical_base: 0x4000,
        reserved_ranges: &[],
        tables: &TEST_DEST_CONFIG.tables[1..2],
    };
    let table = &dest.tables[0];
    let result = build_relocated_bank(&dest, &[source], &rom).unwrap();

    let ptr_offset = table.ptr_bank_offset as usize + 2;
    let ptr_val = u16::from_le_bytes([result[ptr_offset], result[ptr_offset + 1]]);
    // No underflow/wrap: ptr_val must be a small, sane forward offset.
    assert!(ptr_val < 0x100, "ptr_val {ptr_val:#06X} looks wrapped");

    // Reverse the pointer math exactly as the Z80 engine does:
    // bank_offset = base_logical + ptr_val - 0x8000.
    let bank_offset = table.base_logical as usize + ptr_val as usize - 0x8000;
    assert!(
        bank_offset >= (table.base_logical - 0x8000) as usize,
        "text placed below table's min_offset"
    );
    assert_eq!(
        &result[bank_offset..bank_offset + 4],
        &[0xAA, 0xBB, 0xCC, 0xFE]
    );
}

#[test]
fn relocated_two_tables_share_bank_no_ptr_collision() {
    let rom = make_blank_relocation_rom();
    let main_source = RelocatedTableSource {
        source_physical_base: 0,
        entries: &[EncodedEntry {
            id: 1,
            data: vec![0x11, 0x22, 0xFE],
            skip: false,
        }],
    };
    let secondary_source = RelocatedTableSource {
        source_physical_base: 0,
        entries: &[EncodedEntry {
            id: 1,
            data: vec![0x33, 0x44, 0x55, 0xFE],
            skip: false,
        }],
    };
    let result =
        build_relocated_bank(&TEST_DEST_CONFIG, &[main_source, secondary_source], &rom).unwrap();

    // Deterministic layout: secondary (higher base_logical) is packed first
    // and can only use the region at/after its ptr table ($0108+); main
    // backfills the gap before secondary's ptr table (from $0008).
    let ptr_main = u16::from_le_bytes([result[0x02], result[0x03]]) as usize;
    let ptr_secondary = u16::from_le_bytes([result[0x102], result[0x103]]) as usize + 0x100;
    assert_eq!(ptr_main, 0x0008, "main should backfill the low gap");
    assert_eq!(
        ptr_secondary, 0x0108,
        "secondary should start at its own min_offset"
    );

    // No collision: main text and both ptr tables are disjoint.
    assert_eq!(&result[ptr_main..ptr_main + 3], &[0x11, 0x22, 0xFE]);
    assert_eq!(
        &result[ptr_secondary..ptr_secondary + 4],
        &[0x33, 0x44, 0x55, 0xFE]
    );
    // Untouched ptr bytes for other ids stay null.
    assert_eq!(result[0x00], 0x00); // main id0
    assert_eq!(result[0x100], 0x00); // secondary id0
}

#[test]
fn relocated_no_cross_table_suffix_sharing() {
    let rom = make_blank_relocation_rom();
    // secondary's data ends with the exact 2-byte tail [0xBB, 0xFE] that
    // main's (shorter) entry also uses. If cross-table suffix sharing were
    // (incorrectly) enabled, main would resolve into the middle of
    // secondary's allocation instead of getting its own space.
    let main_source = RelocatedTableSource {
        source_physical_base: 0,
        entries: &[EncodedEntry {
            id: 1,
            data: vec![0xBB, 0xFE],
            skip: false,
        }],
    };
    let secondary_source = RelocatedTableSource {
        source_physical_base: 0,
        entries: &[EncodedEntry {
            id: 1,
            data: vec![0xAA, 0xBB, 0xFE],
            skip: false,
        }],
    };
    let result =
        build_relocated_bank(&TEST_DEST_CONFIG, &[main_source, secondary_source], &rom).unwrap();

    let ptr_main = u16::from_le_bytes([result[0x02], result[0x03]]) as usize;
    let ptr_secondary = u16::from_le_bytes([result[0x102], result[0x103]]) as usize + 0x100;

    // If (buggy) cross-table sharing occurred, ptr_main would equal
    // ptr_secondary + 1 (pointing into the middle of secondary's "AA BB FE").
    assert_ne!(
        ptr_main,
        ptr_secondary + 1,
        "main must not share secondary's allocation across tables"
    );
    assert_eq!(ptr_main, 0x0008, "main gets its own fresh allocation");
    assert_eq!(&result[ptr_main..ptr_main + 2], &[0xBB, 0xFE]);
    assert_eq!(
        &result[ptr_secondary..ptr_secondary + 3],
        &[0xAA, 0xBB, 0xFE]
    );
}

#[test]
fn relocated_suffix_sharing_within_table_preserved() {
    let rom = make_test_relocation_rom();
    // Same table: id2's data is an exact suffix of id1's — should share.
    let source = RelocatedTableSource {
        source_physical_base: 0,
        entries: &[
            EncodedEntry {
                id: 1,
                data: vec![0x10, 0x20, 0x30, 0xFE],
                skip: false,
            },
            EncodedEntry {
                id: 2,
                data: vec![0x30, 0xFE],
                skip: false,
            },
        ],
    };
    let dest = MultiBankConfig {
        bank_number: 199,
        physical_base: 0x4000,
        reserved_ranges: &[],
        tables: &TEST_DEST_CONFIG.tables[0..1],
    };
    let result = build_relocated_bank(&dest, &[source], &rom).unwrap();

    let ptr1 = u16::from_le_bytes([result[0x02], result[0x03]]) as usize;
    let ptr2 = u16::from_le_bytes([result[0x04], result[0x05]]) as usize;
    assert_eq!(
        ptr2,
        ptr1 + 2,
        "id2 should share id1's suffix, not allocate fresh"
    );
    assert_eq!(&result[ptr1..ptr1 + 4], &[0x10, 0x20, 0x30, 0xFE]);
}

#[test]
fn relocated_overflow_fails_closed() {
    let rom = make_test_relocation_rom();
    // A single entry larger than the whole 16KB bank can never fit.
    let source = RelocatedTableSource {
        source_physical_base: 0,
        entries: &[EncodedEntry {
            id: 1,
            data: vec![0u8; 20_000],
            skip: false,
        }],
    };
    let dest = MultiBankConfig {
        bank_number: 199,
        physical_base: 0x4000,
        reserved_ranges: &[],
        tables: &TEST_DEST_CONFIG.tables[0..1],
    };
    assert!(build_relocated_bank(&dest, &[source], &rom).is_err());
}

#[test]
fn relocated_bank_matches_verify_whitelist_ptr_protection() {
    // Simulate the region shapes build_relocated_bank hands to
    // verify_bank_within_whitelist (ptr regions zero-initialized, candidate
    // text region = complement) and confirm the reused check still flags a
    // byte that lands outside both sets — the reuse is not a no-op even
    // though, in the real relocated-bank flow, writable ∪ ptr always covers
    // the whole bank by construction (design doc §4.2).
    let bank_size = 0x20usize;
    let ptr_set: HashSet<usize> = (0..4).collect();
    let writable_set: HashSet<usize> = (4..0x10).collect(); // deliberately narrow

    let en_blank = vec![0xFFu8; bank_size];
    let mut bank = en_blank.clone();
    bank[4] = 0x41; // inside writable_set: OK
    bank[1] = 0x00; // inside ptr_set: OK
    bank[0x15] = 0x99; // outside both: must be rejected

    let result = verify_bank_within_whitelist(&bank, &en_blank, &writable_set, &ptr_set, 199);
    assert!(
        result.is_err(),
        "byte outside writable/ptr sets must be rejected"
    );
}

#[test]
fn relocation_config_matches_source() {
    // §1 invariant guard: RELOCATION_MAP's spare configs must duplicate the
    // source SubTable values verbatim, or the pointer math silently breaks.
    for entry in crate::text::relocation::RELOCATION_MAP {
        let source_config = match entry.source_bank {
            3 => &BANK3_CONFIG,
            5 => &BANK5_CONFIG,
            other => panic!("unexpected source_bank {other} in RELOCATION_MAP"),
        };
        let source_table = source_config
            .tables
            .iter()
            .find(|t| t.name == entry.source_table_name)
            .unwrap_or_else(|| {
                panic!(
                    "source table '{}' not found in bank {}",
                    entry.source_table_name, entry.source_bank
                )
            });
        let dest_config = crate::text::relocation::spare_config_for(entry.spare_bank_number)
            .unwrap_or_else(|| panic!("no spare config for bank {}", entry.spare_bank_number));
        let dest_table = dest_config
            .tables
            .iter()
            .find(|t| t.name == entry.source_table_name)
            .unwrap_or_else(|| {
                panic!(
                    "dest table '{}' not found in spare bank {}",
                    entry.source_table_name, entry.spare_bank_number
                )
            });

        assert_eq!(dest_table.base_logical, source_table.base_logical);
        assert_eq!(dest_table.ptr_bank_offset, source_table.ptr_bank_offset);
        assert_eq!(dest_table.ptr_count, source_table.ptr_count);
    }
}
