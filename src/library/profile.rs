//! Exact embedded TypeScript 6.0.3 default-library profile.
//!
//! This module is the production source authority. It admits one ordered 82-file
//! registry and validates every embedded byte before exposing it to the library
//! compiler. Runtime filesystem discovery is deliberately absent.

use crate::source::LibraryFileOrdinal;
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt;

const TYPESCRIPT_VERSION: &str = "6.0.3";
const ROOT_NAME: &str = "lib.es2025.full.d.ts";
const PROFILE_IDENTITY: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const RAW_CONCAT_IDENTITY: &str =
    "0c68516cfe1dff30ce17425b2566813cf6d00c7f589dd24f31f4ba879b69a267";
const PROFILE_MANIFEST_IDENTITY: &str =
    "1edef1b5e870024834762267ec532c3054f3b2279e9181844e21648243eb1407";
const LICENSE_IDENTITY: &str = "a7d00bfd54525bc694b6e32f64c7ebcf5e6b7ae3657be5cc12767bce74654a47";
const NOTICE_IDENTITY: &str = "1af3c68039c57e539422da82a4faada506ce6d0ea6f90e0b699d02dbcdb7a90c";
const SOURCE_BYTES: usize = 2_936_611;

const PROFILE_MANIFEST: &[u8] = include_bytes!("typescript-6.0.3/profile.toml");
const LICENSE_BYTES: &[u8] = include_bytes!("typescript-6.0.3/LICENSE.txt");
const THIRD_PARTY_NOTICE_BYTES: &[u8] = include_bytes!("typescript-6.0.3/ThirdPartyNoticeText.txt");

macro_rules! expected_sources {
    ($(($ordinal:literal, $name:literal, $bytes:literal, $sha256:literal)),+ $(,)?) => {
        const EXPECTED_SOURCES: &[ExpectedSource] = &[
            $(ExpectedSource {
                ordinal: $ordinal,
                name: $name,
                bytes: include_bytes!(concat!("typescript-6.0.3/lib/", $name)),
                expected_bytes: $bytes,
                sha256: $sha256,
            }),+
        ];
    };
}

expected_sources!(
    (
        0,
        "lib.es5.d.ts",
        218972,
        "bcd24271a113971ba9eb71ff8cb01bc6b0f872a85c23fdbe5d93065b375933cd"
    ),
    (
        1,
        "lib.es2015.d.ts",
        1202,
        "3f88bedbeb09c6f5a6645cb24c7c55f1aa22d19ae96c8e6959cbd8b85a707bc6"
    ),
    (
        2,
        "lib.es2016.d.ts",
        926,
        "7fe93b39b810eadd916be8db880dd7f0f7012a5cc6ffb62de8f62a2117fa6f1f"
    ),
    (
        3,
        "lib.es2017.d.ts",
        1123,
        "bb0074cc08b84a2374af33d8bf044b80851ccc9e719a5e202eacf40db2c31600"
    ),
    (
        4,
        "lib.es2018.d.ts",
        1049,
        "1a7daebe4f45fb03d9ec53d60008fbf9ac45a697fdc89e4ce218bc94b94f94d6"
    ),
    (
        5,
        "lib.es2019.d.ts",
        1032,
        "f94b133a3cb14a288803be545ac2683e0d0ff6661bcd37e31aaaec54fc382aed"
    ),
    (
        6,
        "lib.es2020.d.ts",
        1162,
        "f59d0650799f8782fd74cf73c19223730c6d1b9198671b1c5b3a38e1188b5953"
    ),
    (
        7,
        "lib.es2021.d.ts",
        997,
        "8a15b4607d9a499e2dbeed9ec0d3c0d7372c850b2d5f1fb259e8f6d41d468a84"
    ),
    (
        8,
        "lib.es2022.d.ts",
        1069,
        "26e0fe14baee4e127f4365d1ae0b276f400562e45e19e35fd2d4c296684715e6"
    ),
    (
        9,
        "lib.es2023.d.ts",
        960,
        "1e9332c23e9a907175e0ffc6a49e236f97b48838cc8aec9ce7e4cec21e544b65"
    ),
    (
        10,
        "lib.es2024.d.ts",
        1127,
        "3753fbc1113dc511214802a2342280a8b284ab9094f6420e7aa171e868679f91"
    ),
    (
        11,
        "lib.es2025.d.ts",
        1079,
        "999ca32883495a866aa5737fe1babc764a469e4cde6ee6b136a4b9ae68853e4b"
    ),
    (
        12,
        "lib.dom.d.ts",
        2349483,
        "d6b1eba8496bdd0eed6fc8a685768fe01b2da4a0388b5fe7df558290bffcf32f"
    ),
    (
        13,
        "lib.dom.iterable.d.ts",
        933,
        "7f57fc4404ff020bc45b9c620aff2b40f700b95fe31164024c453a5e3c163c54"
    ),
    (
        14,
        "lib.dom.asynciterable.d.ts",
        933,
        "7f57fc4404ff020bc45b9c620aff2b40f700b95fe31164024c453a5e3c163c54"
    ),
    (
        15,
        "lib.webworker.importscripts.d.ts",
        1002,
        "2a2de5b9459b3fc44decd9ce6100b72f1b002ef523126c1d3d8b2a4a63d74d78"
    ),
    (
        16,
        "lib.scripthost.d.ts",
        9412,
        "f13f4b465c99041e912db5c44129a94588e1aafee35a50eab51044833f50b4ee"
    ),
    (
        17,
        "lib.es2015.core.d.ts",
        22824,
        "eadcffda2aa84802c73938e589b9e58248d74c59cb7fcbca6474e3435ac15504"
    ),
    (
        18,
        "lib.es2015.collection.d.ts",
        5305,
        "105ba8ff7ba746404fe1a2e189d1d3d2e0eb29a08c18dded791af02f29fb4711"
    ),
    (
        19,
        "lib.es2015.generator.d.ts",
        2518,
        "00343ca5b2e3d48fa5df1db6e32ea2a59afab09590274a6cccb1dbae82e60c7c"
    ),
    (
        20,
        "lib.es2015.iterable.d.ts",
        18185,
        "ebd9f816d4002697cb2864bea1f0b70a103124e18a8cd9645eeccc09bdf80ab4"
    ),
    (
        21,
        "lib.es2015.promise.d.ts",
        3161,
        "2c1afac30a01772cd2a9a298a7ce7706b5892e447bb46bdbeef720f7b5da77ad"
    ),
    (
        22,
        "lib.es2015.proxy.d.ts",
        5212,
        "7b0225f483e4fa685625ebe43dd584bb7973bbd84e66a6ba7bbe175ee1048b4f"
    ),
    (
        23,
        "lib.es2015.reflect.d.ts",
        6454,
        "c0a4b8ac6ce74679c1da2b3795296f5896e31c38e888469a8e0f99dc3305de60"
    ),
    (
        24,
        "lib.es2015.symbol.d.ts",
        1610,
        "3084a7b5f569088e0146533a00830e206565de65cae2239509168b11434cd84f"
    ),
    (
        25,
        "lib.es2015.symbol.wellknown.d.ts",
        10971,
        "c5079c53f0f141a0698faa903e76cb41cd664e3efb01cc17a5c46ec2eb0bef42"
    ),
    (
        26,
        "lib.es2016.array.include.d.ts",
        5165,
        "32cafbc484dea6b0ab62cf8473182bbcb23020d70845b406f80b7526f38ae862"
    ),
    (
        27,
        "lib.es2016.intl.d.ts",
        1445,
        "fca4cdcb6d6c5ef18a869003d02c9f0fd95df8cfaf6eb431cd3376bc034cad36"
    ),
    (
        28,
        "lib.es2017.arraybuffer.d.ts",
        876,
        "b93ec88115de9a9dc1b602291b85baf825c85666bf25985cc5f698073892b467"
    ),
    (
        29,
        "lib.es2017.date.d.ts",
        1877,
        "f5c06dcc3fe849fcb297c247865a161f995cc29de7aa823afdd75aaaddc1419b"
    ),
    (
        30,
        "lib.es2017.object.d.ts",
        2434,
        "b77e16112127a4b169ef0b8c3a4d730edf459c5f25fe52d5e436a6919206c4d7"
    ),
    (
        31,
        "lib.es2017.sharedmemory.d.ts",
        7170,
        "fbffd9337146eff822c7c00acbb78b01ea7ea23987f6c961eba689349e744f8c"
    ),
    (
        32,
        "lib.es2017.string.d.ts",
        2339,
        "a995c0e49b721312f74fdfb89e4ba29bd9824c770bbb4021d74d2bf560e4c6bd"
    ),
    (
        33,
        "lib.es2017.intl.d.ts",
        1410,
        "c7b3542146734342e440a84b213384bfa188835537ddbda50d30766f0593aff9"
    ),
    (
        34,
        "lib.es2017.typedarrays.d.ts",
        1503,
        "ce6180fa19b1cccd07ee7f7dbb9a367ac19c0ed160573e4686425060b6df7f57"
    ),
    (
        35,
        "lib.es2018.asyncgenerator.d.ts",
        2653,
        "3f02e2476bccb9dbe21280d6090f0df17d2f66b74711489415a8aa4df73c9675"
    ),
    (
        36,
        "lib.es2018.asynciterable.d.ts",
        2218,
        "45e3ab34c1c013c8ab2dc1ba4c80c780744b13b5676800ae2e3be27ae862c40c"
    ),
    (
        37,
        "lib.es2018.intl.d.ts",
        2982,
        "805c86f6cca8d7702a62a844856dbaa2a3fd2abef0536e65d48732441dde5b5b"
    ),
    (
        38,
        "lib.es2018.promise.d.ts",
        1314,
        "e42e397f1a5a77994f0185fd1466520691456c772d06bf843e5084ceb879a0ad"
    ),
    (
        39,
        "lib.es2018.regexp.d.ts",
        1193,
        "f4c2b41f90c95b1c532ecc874bd3c111865793b23aebcc1c3cbbabcd5d76ffb0"
    ),
    (
        40,
        "lib.es2019.array.d.ts",
        3124,
        "ab26191cfad5b66afa11b8bf935ef1cd88fabfcb28d30b2dfa6fad877d050332"
    ),
    (
        41,
        "lib.es2019.object.d.ts",
        1434,
        "2088bc26531e38fb05eedac2951480db5309f6be3fa4a08d2221abb0f5b4200d"
    ),
    (
        42,
        "lib.es2019.string.d.ts",
        1489,
        "cb9d366c425fea79716a8fb3af0d78e6b22ebbab3bd64d25063b42dc9f531c1e"
    ),
    (
        43,
        "lib.es2019.symbol.d.ts",
        967,
        "500934a8089c26d57ebdb688fc9757389bb6207a3c8f0674d68efa900d2abb34"
    ),
    (
        44,
        "lib.es2019.intl.d.ts",
        919,
        "689da16f46e647cef0d64b0def88910e818a5877ca5379ede156ca3afb780ac3"
    ),
    (
        45,
        "lib.es2020.bigint.d.ts",
        38258,
        "bc21cc8b6fee4f4c2440d08035b7ea3c06b3511314c8bab6bef7a92de58a2593"
    ),
    (
        46,
        "lib.es2020.date.d.ts",
        2917,
        "7ca53d13d2957003abb47922a71866ba7cb2068f8d154877c596d63c359fed25"
    ),
    (
        47,
        "lib.es2020.promise.d.ts",
        1764,
        "54725f8c4df3d900cb4dac84b64689ce29548da0b4e9b7c2de61d41c79293611"
    ),
    (
        48,
        "lib.es2020.sharedmemory.d.ts",
        5089,
        "e5594bc3076ac29e6c1ebda77939bc4c8833de72f654b6e376862c0473199323"
    ),
    (
        49,
        "lib.es2020.string.d.ts",
        2451,
        "2f3eb332c2d73e729f3364fcc0c2b375e72a121e8157d25a82d67a138c83a95c"
    ),
    (
        50,
        "lib.es2020.symbol.wellknown.d.ts",
        1566,
        "6f4427f9642ce8d500970e4e69d1397f64072ab73b97e476b4002a646ac743b1"
    ),
    (
        51,
        "lib.es2020.intl.d.ts",
        22399,
        "48915f327cd1dea4d7bd358d9dc7732f58f9e1626a29cc0c05c8c692419d9bb7"
    ),
    (
        52,
        "lib.es2020.number.d.ts",
        1548,
        "b7bf9377723203b5a6a4b920164df22d56a43f593269ba6ae1fdc97774b68855"
    ),
    (
        53,
        "lib.es2021.promise.d.ts",
        2223,
        "db9709688f82c9e5f65a119c64d835f906efe5f559d08b11642d56eb85b79357"
    ),
    (
        54,
        "lib.es2021.string.d.ts",
        1553,
        "4b25b8c874acd1a4cf8444c3617e037d444d19080ac9f634b405583fd10ce1f7"
    ),
    (
        55,
        "lib.es2021.weakref.d.ts",
        3132,
        "37be57d7c90cf1f8112ee2636a068d8fd181289f82b744160ec56a7dc158a9f5"
    ),
    (
        56,
        "lib.es2021.intl.d.ts",
        8206,
        "a917a49ac94cd26b754ab84e113369a75d1a47a710661d7cd25e961cc797065f"
    ),
    (
        57,
        "lib.es2022.array.d.ts",
        4637,
        "6d3261badeb7843d157ef3e6f5d1427d0eeb0af0cf9df84a62cfd29fd47ac86e"
    ),
    (
        58,
        "lib.es2022.error.d.ts",
        2340,
        "195daca651dde22f2167ac0d0a05e215308119a3100f5e6268e8317d05a92526"
    ),
    (
        59,
        "lib.es2022.intl.d.ts",
        7653,
        "8b11e4285cd2bb164a4dc09248bdec69e9842517db4ca47c1ba913011e44ff2f"
    ),
    (
        60,
        "lib.es2022.object.d.ts",
        1046,
        "0508571a52475e245b02bc50fa1394065a0a3d05277fbf5120c3784b85651799"
    ),
    (
        61,
        "lib.es2022.string.d.ts",
        1116,
        "8f9af488f510c3015af3cc8c267a9e9d96c4dd38a1fdff0e11dc5a544711415b"
    ),
    (
        62,
        "lib.es2022.regexp.d.ts",
        1304,
        "fc611fea8d30ea72c6bbfb599c9b4d393ce22e2f5bfef2172534781e7d138104"
    ),
    (
        63,
        "lib.es2023.array.d.ts",
        40197,
        "0bd714129fca875f7d4c477a1a392200b0bcd13fb2e80928cd334b63830ea047"
    ),
    (
        64,
        "lib.es2023.collection.d.ts",
        861,
        "e2c9037ae6cd2c52d80ceef0b3c5ffdb488627d71529cf4f63776daf11161c9a"
    ),
    (
        65,
        "lib.es2023.intl.d.ts",
        2854,
        "135d5cf4d345f59f1a9caadfafcd858d3d9cc68290db616cc85797224448cccc"
    ),
    (
        66,
        "lib.es2024.arraybuffer.d.ts",
        2598,
        "bc238c3f81c2984751932b6aab223cd5b830e0ac6cad76389e5e9d2ffc03287d"
    ),
    (
        67,
        "lib.es2024.collection.d.ts",
        1185,
        "4a07f9b76d361f572620927e5735b77d6d2101c23cdd94383eb5b706e7b36357"
    ),
    (
        68,
        "lib.es2024.object.d.ts",
        1220,
        "7c4e8dc6ab834cc6baa0227e030606d29e3e8449a9f67cdf5605ea5493c4db29"
    ),
    (
        69,
        "lib.es2024.promise.d.ts",
        1350,
        "de7ba0fd02e06cd9a5bd4ab441ed0e122735786e67dde1e849cced1cd8b46b78"
    ),
    (
        70,
        "lib.es2024.regexp.d.ts",
        1034,
        "6148e4e88d720a06855071c3db02069434142a8332cf9c182cda551adedf3156"
    ),
    (
        71,
        "lib.es2024.sharedmemory.d.ts",
        3038,
        "d63dba625b108316a40c95a4425f8d4294e0deeccfd6c7e59d819efa19e23409"
    ),
    (
        72,
        "lib.es2024.string.d.ts",
        1155,
        "0568d6befee03dd435bed4fc25c4e46865b24bdcb8c563fdc21f580a2c301904"
    ),
    (
        73,
        "lib.es2025.collection.d.ts",
        3839,
        "30d62269b05b584741f19a5369852d5d34895aa2ac4fd948956f886d15f9cc0d"
    ),
    (
        74,
        "lib.es2025.float16.d.ts",
        20489,
        "f128dae7c44d8f35ee42e0a437000a57c9f06cc04f8b4fb42eebf44954d53dc8"
    ),
    (
        75,
        "lib.es2025.intl.d.ts",
        9390,
        "ffbe6d7b295306b2ba88030f65b74c107d8d99bdcf596ea99c62a02f606108b0"
    ),
    (
        76,
        "lib.es2025.iterator.d.ts",
        8637,
        "996fb27b15277369c68a4ba46ed138b4e9e839a02fb4ec756f7997629242fd9f"
    ),
    (
        77,
        "lib.es2025.promise.d.ts",
        1619,
        "79b712591b270d4778c89706ca2cfc56ddb8c3f895840e477388f1710dc5eda9"
    ),
    (
        78,
        "lib.es2025.regexp.d.ts",
        1255,
        "20884846cef428b992b9bd032e70a4ef88e349263f63aeddf04dda837a7dba26"
    ),
    (
        79,
        "lib.decorators.d.ts",
        13153,
        "1ce14b81c5cc821994aa8ec1d42b220dd41b27fcc06373bce3958af7421b77d4"
    ),
    (
        80,
        "lib.decorators.legacy.d.ts",
        1287,
        "b3a048b3e9302ef9a34ef4ebb9aecfb28b66abb3bce577206a79fee559c230da"
    ),
    (
        81,
        "lib.es2025.full.d.ts",
        1035,
        "e03da518b01b46a4c99a1f88cd727ee98ddf14492c43dae1ae7a63e992971bab"
    ),
);

struct ExpectedSource {
    ordinal: usize,
    name: &'static str,
    bytes: &'static [u8],
    expected_bytes: usize,
    sha256: &'static str,
}

#[derive(Clone, Debug)]
pub struct ExactLibrarySource {
    ordinal: LibraryFileOrdinal,
    name: String,
    bytes: Cow<'static, [u8]>,
}

impl ExactLibrarySource {
    pub const fn ordinal(&self) -> LibraryFileOrdinal {
        self.ordinal
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Clone, Debug)]
pub struct ExactLibraryProfile {
    sources: Vec<ExactLibrarySource>,
}

impl ExactLibraryProfile {
    pub fn load_packaged() -> Result<Self, LibraryProfileError> {
        let profile = Self::embedded();
        profile.verify_packaged_assets()?;
        Ok(profile)
    }

    fn embedded() -> Self {
        let sources = EXPECTED_SOURCES
            .iter()
            .map(|source| ExactLibrarySource {
                ordinal: LibraryFileOrdinal::new(source.ordinal),
                name: source.name.to_owned(),
                bytes: Cow::Borrowed(source.bytes),
            })
            .collect();
        Self { sources }
    }

    pub fn sources(&self) -> &[ExactLibrarySource] {
        &self.sources
    }

    pub const fn typescript_version(&self) -> &'static str {
        TYPESCRIPT_VERSION
    }

    pub const fn root_name(&self) -> &'static str {
        ROOT_NAME
    }

    pub const fn profile_identity(&self) -> &'static str {
        PROFILE_IDENTITY
    }

    pub const fn license_bytes(&self) -> &'static [u8] {
        LICENSE_BYTES
    }

    pub const fn third_party_notice_bytes(&self) -> &'static [u8] {
        THIRD_PARTY_NOTICE_BYTES
    }

    pub fn verify_packaged_assets(&self) -> Result<(), LibraryProfileError> {
        verify_exact_sources(&self.sources)?;
        verify_asset(
            "profile.toml",
            PROFILE_MANIFEST,
            36_558,
            PROFILE_MANIFEST_IDENTITY,
        )?;
        if PROFILE_MANIFEST.contains(&b'\r') || !PROFILE_MANIFEST.ends_with(b"\n") {
            return Err(LibraryProfileError::AssetIdentityMismatch {
                name: "profile.toml",
            });
        }
        verify_asset("LICENSE.txt", LICENSE_BYTES, 9_197, LICENSE_IDENTITY)?;
        verify_asset(
            "ThirdPartyNoticeText.txt",
            THIRD_PARTY_NOTICE_BYTES,
            37_824,
            NOTICE_IDENTITY,
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn load_packaged_with_source_override_for_test(
        name: &str,
        bytes: &[u8],
    ) -> Result<Self, LibraryProfileError> {
        let mut profile = Self::embedded();
        let source = profile
            .sources
            .iter_mut()
            .find(|source| source.name == name)
            .ok_or_else(|| LibraryProfileError::UnknownSource {
                name: name.to_owned(),
            })?;
        source.bytes = Cow::Owned(bytes.to_vec());
        profile.verify_packaged_assets()?;
        Ok(profile)
    }

    #[cfg(test)]
    pub(crate) fn test_input_with_appended_source(
        &self,
        name: &str,
        bytes: &[u8],
    ) -> Result<TestLibraryProfileInput, LibraryProfileError> {
        validate_test_name(name)?;
        if self.sources.iter().any(|source| source.name == name) {
            return Err(LibraryProfileError::DuplicateSource {
                name: name.to_owned(),
            });
        }
        let mut sources = self.sources.clone();
        sources.push(ExactLibrarySource {
            ordinal: LibraryFileOrdinal::new(sources.len()),
            name: name.to_owned(),
            bytes: Cow::Owned(bytes.to_vec()),
        });
        let profile_identity = framed_identity(&sources)?;
        Ok(TestLibraryProfileInput {
            sources,
            profile_identity,
        })
    }
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct TestLibraryProfileInput {
    sources: Vec<ExactLibrarySource>,
    profile_identity: String,
}

#[cfg(test)]
impl TestLibraryProfileInput {
    pub(crate) fn sources(&self) -> &[ExactLibrarySource] {
        &self.sources
    }

    pub(crate) fn profile_identity(&self) -> &str {
        &self.profile_identity
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryProfileError {
    SourceCountMismatch { expected: usize, actual: usize },
    SourceOrderMismatch { position: usize, name: String },
    DuplicateSource { name: String },
    UnknownSource { name: String },
    SourceIdentityMismatch { name: String },
    AggregateIdentityMismatch { authority: &'static str },
    AssetIdentityMismatch { name: &'static str },
    InvalidSourceName { name: String },
    LengthOverflow,
}

impl fmt::Display for LibraryProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceCountMismatch { expected, actual } => {
                write!(
                    formatter,
                    "expected {expected} library sources, found {actual}"
                )
            }
            Self::SourceOrderMismatch { position, name } => {
                write!(
                    formatter,
                    "unexpected library source {name:?} at position {position}"
                )
            }
            Self::DuplicateSource { name } => {
                write!(formatter, "duplicate library source {name:?}")
            }
            Self::UnknownSource { name } => write!(formatter, "unknown library source {name:?}"),
            Self::SourceIdentityMismatch { name } => {
                write!(formatter, "library source identity differs for {name:?}")
            }
            Self::AggregateIdentityMismatch { authority } => {
                write!(formatter, "library {authority} identity differs")
            }
            Self::AssetIdentityMismatch { name } => {
                write!(
                    formatter,
                    "packaged library asset identity differs for {name}"
                )
            }
            Self::InvalidSourceName { name } => {
                write!(formatter, "invalid library source name {name:?}")
            }
            Self::LengthOverflow => {
                formatter.write_str("library profile length exceeds its wire domain")
            }
        }
    }
}

impl std::error::Error for LibraryProfileError {}

fn verify_exact_sources(sources: &[ExactLibrarySource]) -> Result<(), LibraryProfileError> {
    if sources.len() != EXPECTED_SOURCES.len() {
        return Err(LibraryProfileError::SourceCountMismatch {
            expected: EXPECTED_SOURCES.len(),
            actual: sources.len(),
        });
    }
    let mut names = BTreeSet::new();
    let mut total_bytes = 0usize;
    let mut raw = Sha256::new();
    for (position, (source, expected)) in sources.iter().zip(EXPECTED_SOURCES).enumerate() {
        if !names.insert(source.name()) {
            return Err(LibraryProfileError::DuplicateSource {
                name: source.name().to_owned(),
            });
        }
        if source.ordinal().index() != position
            || expected.ordinal != position
            || source.name() != expected.name
        {
            return Err(LibraryProfileError::SourceOrderMismatch {
                position,
                name: source.name().to_owned(),
            });
        }
        if source.bytes().len() != expected.expected_bytes
            || sha256_hex(source.bytes()) != expected.sha256
        {
            return Err(LibraryProfileError::SourceIdentityMismatch {
                name: source.name().to_owned(),
            });
        }
        total_bytes = total_bytes
            .checked_add(source.bytes().len())
            .ok_or(LibraryProfileError::LengthOverflow)?;
        raw.update(source.bytes());
    }
    if total_bytes != SOURCE_BYTES || format!("{:x}", raw.finalize()) != RAW_CONCAT_IDENTITY {
        return Err(LibraryProfileError::AggregateIdentityMismatch {
            authority: "raw-concatenation",
        });
    }
    if framed_identity(sources)? != PROFILE_IDENTITY {
        return Err(LibraryProfileError::AggregateIdentityMismatch {
            authority: "length-framed registry",
        });
    }
    Ok(())
}

fn framed_identity(sources: &[ExactLibrarySource]) -> Result<String, LibraryProfileError> {
    let mut framed = Sha256::new();
    for source in sources {
        let name_len =
            u64::try_from(source.name().len()).map_err(|_| LibraryProfileError::LengthOverflow)?;
        let source_len =
            u64::try_from(source.bytes().len()).map_err(|_| LibraryProfileError::LengthOverflow)?;
        framed.update(name_len.to_be_bytes());
        framed.update(source_len.to_be_bytes());
        framed.update(source.name().as_bytes());
        framed.update(source.bytes());
    }
    Ok(format!("{:x}", framed.finalize()))
}

fn verify_asset(
    name: &'static str,
    bytes: &[u8],
    expected_bytes: usize,
    expected_sha256: &str,
) -> Result<(), LibraryProfileError> {
    if bytes.len() != expected_bytes || sha256_hex(bytes) != expected_sha256 {
        return Err(LibraryProfileError::AssetIdentityMismatch { name });
    }
    Ok(())
}

#[cfg(test)]
fn validate_test_name(name: &str) -> Result<(), LibraryProfileError> {
    if name.is_empty()
        || !name.ends_with(".d.ts")
        || name.contains('/')
        || name.contains('\\')
        || name == ".d.ts"
    {
        return Err(LibraryProfileError::InvalidSourceName {
            name: name.to_owned(),
        });
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_validator_rejects_cardinality_duplicate_order_and_byte_drift() {
        let profile = ExactLibraryProfile::embedded();

        let mut extra = profile.sources.clone();
        extra.push(extra[0].clone());
        assert!(matches!(
            verify_exact_sources(&extra),
            Err(LibraryProfileError::SourceCountMismatch { .. })
        ));

        let mut missing = profile.sources.clone();
        missing.pop();
        assert!(matches!(
            verify_exact_sources(&missing),
            Err(LibraryProfileError::SourceCountMismatch { .. })
        ));

        let mut duplicate = profile.sources.clone();
        duplicate[1].name = duplicate[0].name.clone();
        assert!(matches!(
            verify_exact_sources(&duplicate),
            Err(LibraryProfileError::DuplicateSource { .. })
        ));

        let mut reordered = profile.sources.clone();
        reordered.swap(0, 1);
        assert!(matches!(
            verify_exact_sources(&reordered),
            Err(LibraryProfileError::SourceOrderMismatch { .. })
        ));

        let mut changed = profile.sources.clone();
        changed[0].bytes = Cow::Owned(b"interface Changed {}\n".to_vec());
        assert!(matches!(
            verify_exact_sources(&changed),
            Err(LibraryProfileError::SourceIdentityMismatch { .. })
        ));
    }
}
