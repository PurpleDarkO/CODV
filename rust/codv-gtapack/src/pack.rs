//! CODV's DLC pack, `update/x64/dlcpacks/codv/dlc.rpf` (mounted by the host,
//! host/src/pack_loader.h): WEAPON_CODV_MW2's weapon meta (written by
//! tools/build-mw2-weapon.py from the player's own update.rpf) and its name.
//! OPEN (clear) archives, laid out like SK8V's skatev pack
//! (SK8V rust/ped-export/src/bin/live_clip.rs, Apache-2.0, NOTICE) without
//! its animation archive.

use anyhow::{Result, ensure};
use rpf_archive::crypto::jenkins_hash;
use rpf_archive::{RpfBuilder, RpfEncryption};

pub const SETUP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<SSetupData>
  <deviceName>dlc_codv</deviceName>
  <datFile>content.xml</datFile>
  <timeStamp>10/18/2026 00:00:00</timeStamp>
  <nameHash>codv</nameHash>
  <contentChangeSetGroups>
    <Item>
      <NameHash>GROUP_STARTUP</NameHash>
      <ContentChangeSets>
        <Item>CODV_AUTOGEN</Item>
      </ContentChangeSets>
    </Item>
  </contentChangeSetGroups>
  <type>EXTRACONTENT_COMPAT_PACK</type>
  <order value="14" />
</SSetupData>
"#;

/// Weapon data files of the pack: file name and GTA data file type.
pub const WEAPON_FILES: [(&str, &str); 2] =
    [("weapons_codv.meta", "WEAPONINFO_FILE"), ("weaponanimations_codv.meta", "WEAPON_ANIMATIONS_FILE")];

/// The name the wheel shows (HumanNameHash of the weapon).
/// ponytail: English only; other languages show the raw label until copies are packed.
pub const TEXT: [(&str, &str); 1] = [("WT_CODV_MW2", "MW2")];

const DLCTEXT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<CExtraTextMetaFile>
	<hasGlobalTextFile value="true"/>
	<hasAdditionalText value="false"/>
	<isTitleUpdate value="false"/>
</CExtraTextMetaFile>
"#;

fn item(filename: &str, kind: &str, overlay: bool) -> String {
    format!(
        "
    <Item>
      <filename>{filename}</filename>
      <fileType>{kind}</fileType>
      <overlay value=\"{overlay}\" />
      <disabled value=\"true\" />
      <persistent value=\"true\" />
    </Item>"
    )
}

pub fn content() -> String {
    let mut files = String::new();
    let mut enable = String::new();
    let mut add = |path: String, kind: &str, overlay: bool| {
        files += &item(&path, kind, overlay);
        enable += &format!("\n        <Item>{path}</Item>");
    };
    for (name, kind) in WEAPON_FILES {
        add(format!("dlc_codv:/common/data/ai/{name}"), kind, true);
    }
    add("dlc_codv:/common/data/dlctext.meta".into(), "TEXTFILE_METAFILE", false);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<CDataFileMgr__ContentsOfDataFileXml>
  <disabledFiles />
  <includedXmlFiles />
  <includedDataFiles />
  <dataFiles>{files}
  </dataFiles>
  <contentChangeSets>
    <Item>
      <changeSetName>CODV_AUTOGEN</changeSetName>
      <mapChangeSetData />
      <filesToInvalidate />
      <filesToDisable />
      <filesToEnable>{enable}
      </filesToEnable>
      <txdToLoad />
      <txdToUnload />
      <residentResources />
      <unregisterResources />
    </Item>
  </contentChangeSets>
  <patchFiles />
</CDataFileMgr__ContentsOfDataFileXml>
"#
    )
}

/// GXT2 as in retail `global.gxt2`: "2TXG", count, (label hash, string offset) sorted by
/// hash, "2TXG", file size, then the NUL-terminated UTF-8 strings.
pub fn gxt2(text: &[(&str, &str)]) -> Vec<u8> {
    let mut rows: Vec<(u32, &str)> = text.iter().map(|(k, v)| (jenkins_hash(k), *v)).collect();
    rows.sort();
    let mut strings = Vec::new();
    let base = 16 + 8 * rows.len();
    let mut out = b"2TXG".to_vec();
    out.extend((rows.len() as u32).to_le_bytes());
    for (hash, s) in &rows {
        out.extend(hash.to_le_bytes());
        out.extend(((base + strings.len()) as u32).to_le_bytes());
        strings.extend(s.as_bytes());
        strings.push(0);
    }
    out.extend(b"2TXG");
    out.extend(((base + strings.len()) as u32).to_le_bytes());
    out.extend(strings);
    out
}

/// The pack: setup2.xml, content.xml, the weapon meta and the name text.
pub fn dlc(weapon: &[(&str, Vec<u8>)]) -> Result<Vec<u8>> {
    ensure!(weapon.len() == WEAPON_FILES.len(), "the pack needs both weapon meta files");
    let mut outer = RpfBuilder::new(RpfEncryption::Open);
    outer.add_file("setup2.xml", SETUP.as_bytes().to_vec());
    outer.add_file("content.xml", content().into_bytes());
    for (name, bytes) in weapon {
        outer.add_file(&format!("common/data/ai/{name}"), bytes.clone());
    }
    let mut lang = RpfBuilder::new(RpfEncryption::Open);
    lang.add_file("global.gxt2", gxt2(&TEXT));
    outer.add_file("common/data/dlctext.meta", DLCTEXT.as_bytes().to_vec());
    outer.add_file("x64/data/lang/americandlc.rpf", stored(lang.build(None)?)?);
    stored(outer.build(None)?)
}

/// The builder writes a stored binary file's compressed size as its size, but
/// GTA treats any non-zero compressed size as deflated and refuses the pack
/// (ERR_FIL_PACK_3). Stored must be 0, as in the NFS `iverson` pack (SK8V).
pub fn stored(mut rpf: Vec<u8>) -> Result<Vec<u8>> {
    let word = |b: &[u8], at: usize| u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    ensure!(word(&rpf, 0) == 0x5250_4637 && word(&rpf, 12) == 0x4e45_504f, "not an OPEN RPF7");
    for i in 0..word(&rpf, 4) as usize {
        let e = 16 + 16 * i;
        let directory = word(&rpf, e + 4) == 0x7fff_ff00;
        let resource = rpf[e + 7] & 0x80 != 0;
        if !directory && !resource {
            rpf[e + 2..e + 5].fill(0);
        }
    }
    Ok(rpf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_lists_and_enables_every_file() {
        let c = content();
        for (name, kind) in WEAPON_FILES {
            let path = format!("dlc_codv:/common/data/ai/{name}");
            assert_eq!(c.matches(&path).count(), 2, "{name}: data file + filesToEnable");
            assert!(c.contains(&format!("<fileType>{kind}</fileType>")));
        }
        assert_eq!(c.matches("dlc_codv:/common/data/dlctext.meta").count(), 2);
        assert!(!c.contains('{'));
    }

    #[test]
    fn gxt2_matches_the_retail_layout() {
        assert_eq!(jenkins_hash("WEAPON_SKATEBOARD"), 0x38A8_F85C);
        let g = gxt2(&[("B", "two"), ("A", "one")]);
        let word = |at: usize| u32::from_le_bytes(g[at..at + 4].try_into().unwrap());
        assert_eq!((&g[..4], word(4), &g[24..28], word(28) as usize), (&b"2TXG"[..], 2, &b"2TXG"[..], g.len()));
        assert!(word(8) < word(16), "rows sorted by hash");
    }

    #[test]
    fn the_pack_is_an_open_rpf7_with_its_files() {
        let pack =
            dlc(&[("weapons_codv.meta", b"<a/>".to_vec()), ("weaponanimations_codv.meta", b"<b/>".to_vec())]).unwrap();
        assert_eq!(&pack[..4], b"7FPR");
        let a = rpf_archive::RpfArchive::parse(&pack, "dlc.rpf", None).unwrap();
        let names: Vec<&str> = a.entries.iter().map(|e| e.name.as_str()).collect();
        for n in ["setup2.xml", "content.xml", "weapons_codv.meta", "dlctext.meta", "americandlc.rpf"] {
            assert!(names.contains(&n), "{n} in {names:?}");
        }
    }
}
