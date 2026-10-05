#[cfg(not(feature = "bundled-resources"))]
use simtower_formats::{DibImage, NeFile, trim_wave};
#[cfg(not(feature = "bundled-resources"))]
use std::{collections::HashMap, env, fs, path::PathBuf, process::Command, sync::OnceLock};

#[cfg(not(feature = "bundled-resources"))]
const RT_BITMAP: u16 = 2;
#[cfg(not(feature = "bundled-resources"))]
const RT_ICON: u16 = 3;
#[cfg(not(feature = "bundled-resources"))]
const SIMTOWER_RAW_BITMAP_TYPE: u16 = 32_514;
#[cfg(not(feature = "bundled-resources"))]
const SIMTOWER_PALETTE_TYPE: u16 = 32_515;
#[cfg(not(feature = "bundled-resources"))]
const SIMTOWER_BASE_PALETTE_ID: u16 = 1_000;
#[cfg(not(feature = "bundled-resources"))]
const SIMTOWER_WAVE_TYPE: u16 = 32_522;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum AssetKind {
    Bitmap,
    RawBitmap,
    Sound,
    Icon,
}

#[derive(Clone, Copy)]
pub(crate) struct AssetRef {
    #[cfg(not(feature = "bundled-resources"))]
    kind: AssetKind,
    #[cfg(not(feature = "bundled-resources"))]
    id: u16,
    #[cfg(feature = "bundled-resources")]
    embedded: &'static [u8],
}

impl AssetRef {
    #[cfg(feature = "bundled-resources")]
    const fn bundled(_kind: AssetKind, _id: u16, embedded: &'static [u8]) -> Self {
        Self { embedded }
    }

    #[cfg(not(feature = "bundled-resources"))]
    const fn external(kind: AssetKind, id: u16) -> Self {
        Self { kind, id }
    }

    pub(crate) fn bytes(self) -> Result<&'static [u8], String> {
        #[cfg(feature = "bundled-resources")]
        {
            Ok(self.embedded)
        }
        #[cfg(not(feature = "bundled-resources"))]
        {
            ORIGINAL_RESOURCES
                .get()
                .ok_or_else(|| "original SimTower resources have not been loaded".to_owned())?
                .assets
                .get(&(self.kind, self.id))
                .map(Vec::as_slice)
                .ok_or_else(|| {
                    format!(
                        "SimTower.exe is missing {:?} resource {}",
                        self.kind, self.id
                    )
                })
        }
    }
}

#[cfg(feature = "bundled-resources")]
macro_rules! bitmap_asset {
    ($id:literal) => {
        AssetRef::bundled(
            AssetKind::Bitmap,
            $id,
            include_bytes!(concat!(
                "../../../assets/original/bitmaps/",
                stringify!($id),
                ".bmp"
            )),
        )
    };
}

#[cfg(not(feature = "bundled-resources"))]
macro_rules! bitmap_asset {
    ($id:literal) => {
        AssetRef::external(AssetKind::Bitmap, $id)
    };
}

#[cfg(feature = "bundled-resources")]
macro_rules! raw_bitmap_asset {
    ($id:literal) => {
        AssetRef::bundled(
            AssetKind::RawBitmap,
            $id,
            include_bytes!(concat!(
                "../../../assets/original/raw-bitmaps/",
                stringify!($id),
                ".bmp"
            )),
        )
    };
}

#[cfg(not(feature = "bundled-resources"))]
macro_rules! raw_bitmap_asset {
    ($id:literal) => {
        AssetRef::external(AssetKind::RawBitmap, $id)
    };
}

#[cfg(feature = "bundled-resources")]
macro_rules! sound_asset {
    ($id:literal) => {
        AssetRef::bundled(
            AssetKind::Sound,
            $id,
            include_bytes!(concat!(
                "../../../assets/original/sounds/",
                stringify!($id),
                ".wav"
            )),
        )
    };
}

#[cfg(not(feature = "bundled-resources"))]
macro_rules! sound_asset {
    ($id:literal) => {
        AssetRef::external(AssetKind::Sound, $id)
    };
}

#[cfg(feature = "bundled-resources")]
macro_rules! icon_asset {
    ($id:literal) => {
        AssetRef::bundled(
            AssetKind::Icon,
            $id,
            include_bytes!(concat!(
                "../../../assets/original/icons/",
                stringify!($id),
                ".bmp"
            )),
        )
    };
}

pub(crate) const SKY_BMP: AssetRef = bitmap_asset!(352);
pub(crate) const GROUND_BMP: AssetRef = bitmap_asset!(849);
pub(crate) const PALETTE_BMP: AssetRef = bitmap_asset!(300);
pub(crate) const PALETTE_SELECTED_BMP: AssetRef = bitmap_asset!(301);
pub(crate) const PALETTE_DISABLED_BMP: AssetRef = bitmap_asset!(302);
pub(crate) const PLAY_BMP: AssetRef = bitmap_asset!(600);
pub(crate) const PLAY_SELECTED_BMP: AssetRef = bitmap_asset!(601);
pub(crate) const PAUSE_BMP: AssetRef = bitmap_asset!(602);
pub(crate) const PAUSE_SELECTED_BMP: AssetRef = bitmap_asset!(603);
pub(crate) const POINTER_TOOLS_BMP: AssetRef = bitmap_asset!(604);
pub(crate) const POINTER_TOOLS_SELECTED_BMP: AssetRef = bitmap_asset!(605);
pub(crate) const SCAFFOLD_BMP: AssetRef = bitmap_asset!(3624);
pub(crate) const FLOOR_STRIP_BMP: AssetRef = bitmap_asset!(5000);
pub(crate) const EMERGENCY_STAIRS_BMP: AssetRef = bitmap_asset!(1069);
pub(crate) const ROOF_CRANE_BMP: AssetRef = bitmap_asset!(1002);
pub(crate) const LOBBY_FIRST_STORY_BMPS: [AssetRef; 3] = [
    raw_bitmap_asset!(2536),
    raw_bitmap_asset!(2537),
    raw_bitmap_asset!(2538),
];
pub(crate) const LOBBY_SECOND_STORY_BMPS: [AssetRef; 3] = [
    raw_bitmap_asset!(2600),
    raw_bitmap_asset!(2601),
    raw_bitmap_asset!(2602),
];
pub(crate) const LOBBY_THIRD_STORY_BMPS: [AssetRef; 3] = [
    raw_bitmap_asset!(2664),
    raw_bitmap_asset!(2665),
    raw_bitmap_asset!(2666),
];
pub(crate) const LOBBY_AWNING_BMP: AssetRef = bitmap_asset!(1001);
pub(crate) const SANTA_BMP: AssetRef = bitmap_asset!(904);
pub(crate) const TREASURE_BMP: AssetRef = bitmap_asset!(10003);
pub(crate) const HOTEL_SINGLE_1_BMP: AssetRef = bitmap_asset!(1192);
pub(crate) const HOTEL_SINGLE_2_BMP: AssetRef = bitmap_asset!(1194);
pub(crate) const HOTEL_SINGLE_OCCUPIED_1_BMP: AssetRef = bitmap_asset!(1193);
pub(crate) const HOTEL_SINGLE_OCCUPIED_2_BMP: AssetRef = bitmap_asset!(1195);
pub(crate) const HOTEL_TWIN_1_BMP: AssetRef = bitmap_asset!(1256);
pub(crate) const HOTEL_TWIN_2_BMP: AssetRef = bitmap_asset!(1258);
pub(crate) const HOTEL_TWIN_3_BMP: AssetRef = bitmap_asset!(1260);
pub(crate) const HOTEL_TWIN_4_BMP: AssetRef = bitmap_asset!(1262);
pub(crate) const HOTEL_TWIN_OCCUPIED_1_BMP: AssetRef = bitmap_asset!(1257);
pub(crate) const HOTEL_TWIN_OCCUPIED_2_BMP: AssetRef = bitmap_asset!(1259);
pub(crate) const HOTEL_TWIN_OCCUPIED_3_BMP: AssetRef = bitmap_asset!(1261);
pub(crate) const HOTEL_TWIN_OCCUPIED_4_BMP: AssetRef = bitmap_asset!(1263);
pub(crate) const HOTEL_SUITE_1_BMP: AssetRef = bitmap_asset!(1320);
pub(crate) const HOTEL_SUITE_2_BMP: AssetRef = bitmap_asset!(1322);
pub(crate) const HOTEL_SUITE_OCCUPIED_1_BMP: AssetRef = bitmap_asset!(1321);
pub(crate) const HOTEL_SUITE_OCCUPIED_2_BMP: AssetRef = bitmap_asset!(1323);
pub(crate) const OFFICE_1_BMP: AssetRef = bitmap_asset!(1448);
pub(crate) const OFFICE_2_BMP: AssetRef = bitmap_asset!(1449);
pub(crate) const OFFICE_3_BMP: AssetRef = bitmap_asset!(1450);
pub(crate) const OFFICE_VACANT_BMP: AssetRef = bitmap_asset!(1451);
pub(crate) const CONDO_STATE_BMPS: [AssetRef; 15] = [
    bitmap_asset!(1576),
    bitmap_asset!(1577),
    bitmap_asset!(1578),
    bitmap_asset!(1579),
    bitmap_asset!(1580),
    bitmap_asset!(1581),
    bitmap_asset!(1582),
    bitmap_asset!(1583),
    bitmap_asset!(1584),
    bitmap_asset!(1585),
    bitmap_asset!(1586),
    bitmap_asset!(1587),
    bitmap_asset!(1588),
    bitmap_asset!(1589),
    bitmap_asset!(1590),
];
pub(crate) const RESTAURANT_1_BMP: AssetRef = bitmap_asset!(1384);
pub(crate) const RESTAURANT_2_BMP: AssetRef = bitmap_asset!(1386);
pub(crate) const RESTAURANT_3_BMP: AssetRef = bitmap_asset!(1388);
pub(crate) const RESTAURANT_4_BMP: AssetRef = bitmap_asset!(1390);
pub(crate) const RESTAURANT_5_BMP: AssetRef = bitmap_asset!(1392);
pub(crate) const RESTAURANT_OCCUPIED_1_BMP: AssetRef = bitmap_asset!(1385);
pub(crate) const RESTAURANT_OCCUPIED_2_BMP: AssetRef = bitmap_asset!(1387);
pub(crate) const RESTAURANT_OCCUPIED_3_BMP: AssetRef = bitmap_asset!(1389);
pub(crate) const RESTAURANT_OCCUPIED_4_BMP: AssetRef = bitmap_asset!(1391);
pub(crate) const RESTAURANT_OCCUPIED_5_BMP: AssetRef = bitmap_asset!(1393);
pub(crate) const FAST_FOOD_1_BMP: AssetRef = bitmap_asset!(1768);
pub(crate) const FAST_FOOD_2_BMP: AssetRef = bitmap_asset!(1770);
pub(crate) const FAST_FOOD_3_BMP: AssetRef = bitmap_asset!(1772);
pub(crate) const FAST_FOOD_4_BMP: AssetRef = bitmap_asset!(1774);
pub(crate) const FAST_FOOD_5_BMP: AssetRef = bitmap_asset!(1776);
pub(crate) const FAST_FOOD_OCCUPIED_1_BMP: AssetRef = bitmap_asset!(1769);
pub(crate) const FAST_FOOD_OCCUPIED_2_BMP: AssetRef = bitmap_asset!(1771);
pub(crate) const FAST_FOOD_OCCUPIED_3_BMP: AssetRef = bitmap_asset!(1773);
pub(crate) const FAST_FOOD_OCCUPIED_4_BMP: AssetRef = bitmap_asset!(1775);
pub(crate) const FAST_FOOD_OCCUPIED_5_BMP: AssetRef = bitmap_asset!(1777);
pub(crate) const SHOP_1_BMP: AssetRef = bitmap_asset!(1640);
pub(crate) const SHOP_2_BMP: AssetRef = bitmap_asset!(1641);
pub(crate) const SHOP_3_BMP: AssetRef = bitmap_asset!(1642);
pub(crate) const SHOP_4_BMP: AssetRef = bitmap_asset!(1643);
pub(crate) const SHOP_5_BMP: AssetRef = bitmap_asset!(1644);
pub(crate) const SHOP_6_BMP: AssetRef = bitmap_asset!(1645);
pub(crate) const SHOP_7_BMP: AssetRef = bitmap_asset!(1646);
pub(crate) const SHOP_8_BMP: AssetRef = bitmap_asset!(1647);
pub(crate) const SHOP_9_BMP: AssetRef = bitmap_asset!(1648);
pub(crate) const SHOP_10_BMP: AssetRef = bitmap_asset!(1649);
pub(crate) const SHOP_11_BMP: AssetRef = bitmap_asset!(1650);
pub(crate) const PARKING_BMP: AssetRef = bitmap_asset!(1704);
pub(crate) const MEDICAL_BMP: AssetRef = bitmap_asset!(1832);
pub(crate) const SECURITY_BMP: AssetRef = bitmap_asset!(1896);
pub(crate) const HOUSEKEEPING_BMP: AssetRef = bitmap_asset!(1960);
pub(crate) const ELEVATOR_BMP: AssetRef = bitmap_asset!(1064);
pub(crate) const ELEVATOR_CARS_BMP: AssetRef = bitmap_asset!(1065);
pub(crate) const SERVICE_ELEVATOR_BMP: AssetRef = bitmap_asset!(1066);
pub(crate) const EXPRESS_ELEVATOR_BMP: AssetRef = bitmap_asset!(1067);
pub(crate) const ELEVATOR_SHAFT_BMP: AssetRef = bitmap_asset!(1068);
pub(crate) const PEOPLE_BMPS: [AssetRef; 7] = [
    bitmap_asset!(1512),
    bitmap_asset!(1513),
    bitmap_asset!(1514),
    bitmap_asset!(1515),
    bitmap_asset!(1516),
    bitmap_asset!(1517),
    bitmap_asset!(1518),
];
pub(crate) const PEOPLE_SILHOUETTE_BMPS: [AssetRef; 2] = [bitmap_asset!(1128), bitmap_asset!(1129)];
pub(crate) const QUEUE_PEOPLE_BMPS: [AssetRef; 4] = [
    bitmap_asset!(700),
    bitmap_asset!(703),
    bitmap_asset!(701),
    bitmap_asset!(702),
];
pub(crate) const ELEVATOR_NUMBER_BMPS: [AssetRef; 6] = [
    bitmap_asset!(2024),
    bitmap_asset!(2025),
    bitmap_asset!(2026),
    bitmap_asset!(2027),
    bitmap_asset!(2028),
    bitmap_asset!(2029),
];
pub(crate) const CINEMA_UPPER_BMP: AssetRef = bitmap_asset!(2152);
pub(crate) const CINEMA_LOWER_BMP: AssetRef = bitmap_asset!(2216);
pub(crate) const RECYCLING_BMP: AssetRef = bitmap_asset!(2280);
pub(crate) const STAIRS_UPPER_BMP: AssetRef = bitmap_asset!(2408);
pub(crate) const STAIRS_LOWER_BMP: AssetRef = bitmap_asset!(2472);
pub(crate) const ESCALATOR_UPPER_BMP: AssetRef = bitmap_asset!(2728);
pub(crate) const ESCALATOR_LOWER_BMP: AssetRef = bitmap_asset!(2792);
pub(crate) const PARTY_HALL_UPPER_BMP: AssetRef = bitmap_asset!(2856);
pub(crate) const PARTY_HALL_LOWER_BMP: AssetRef = bitmap_asset!(2920);
pub(crate) const METRO_UPPER_BMP: AssetRef = bitmap_asset!(2984);
pub(crate) const METRO_MIDDLE_BMP: AssetRef = bitmap_asset!(3048);
pub(crate) const METRO_LOWER_BMP: AssetRef = bitmap_asset!(3112);
pub(crate) const METRO_OCCUPIED_UPPER_BMP: AssetRef = bitmap_asset!(2985);
pub(crate) const METRO_OCCUPIED_MIDDLE_BMP: AssetRef = bitmap_asset!(3049);
pub(crate) const METRO_OCCUPIED_LOWER_BMP: AssetRef = bitmap_asset!(3113);
pub(crate) const CATHEDRAL_1_BMP: AssetRef = bitmap_asset!(3304);
pub(crate) const CATHEDRAL_2_BMP: AssetRef = bitmap_asset!(3368);
pub(crate) const CATHEDRAL_3_BMP: AssetRef = bitmap_asset!(3432);
pub(crate) const CATHEDRAL_4_BMP: AssetRef = bitmap_asset!(3496);
pub(crate) const CATHEDRAL_5_BMP: AssetRef = bitmap_asset!(3560);
pub(crate) const RAMP_BMP: AssetRef = bitmap_asset!(3816);
pub(crate) const RAMP_2_BMP: AssetRef = bitmap_asset!(3817);
pub(crate) const RAMP_3_BMP: AssetRef = bitmap_asset!(3818);
pub(crate) const PARKING_OCCUPIED_BMP: AssetRef = bitmap_asset!(1705);
pub(crate) const MEDICAL_2_BMP: AssetRef = bitmap_asset!(1833);
pub(crate) const MEDICAL_3_BMP: AssetRef = bitmap_asset!(1834);
pub(crate) const RECYCLING_LEVEL_BMPS: [AssetRef; 6] = [
    bitmap_asset!(2280),
    bitmap_asset!(2281),
    bitmap_asset!(2282),
    bitmap_asset!(2283),
    bitmap_asset!(2284),
    bitmap_asset!(2285),
];
pub(crate) const RECYCLING_TRUCK_BMP: AssetRef = bitmap_asset!(2350);
pub(crate) const RAIN_BMPS: [AssetRef; 10] = [
    bitmap_asset!(850),
    bitmap_asset!(851),
    bitmap_asset!(852),
    bitmap_asset!(853),
    bitmap_asset!(854),
    bitmap_asset!(855),
    bitmap_asset!(856),
    bitmap_asset!(857),
    bitmap_asset!(858),
    bitmap_asset!(859),
];
pub(crate) const CLOUD_BMPS: [AssetRef; 4] = [
    bitmap_asset!(900),
    bitmap_asset!(901),
    bitmap_asset!(902),
    bitmap_asset!(903),
];
pub(crate) const CITY_BMP: AssetRef = bitmap_asset!(905);
pub(crate) const INTRO_STORE_BMP: AssetRef = bitmap_asset!(256);
pub(crate) const INTRO_TITLE_BMP: AssetRef = bitmap_asset!(257);
pub(crate) const INTRO_MAXIS_BMP: AssetRef = bitmap_asset!(128);
pub(crate) const STAR_ON_BMP: AssetRef = bitmap_asset!(322);
pub(crate) const STAR_OFF_BMP: AssetRef = bitmap_asset!(323);
pub(crate) const TOWER_LOGO_BMP: AssetRef = bitmap_asset!(327);
pub(crate) const FINANCE_DIALOG_BMP: AssetRef = bitmap_asset!(500);
pub(crate) const FIRE_LARGE_BMPS: [AssetRef; 4] = [
    bitmap_asset!(3944),
    bitmap_asset!(3945),
    bitmap_asset!(3946),
    bitmap_asset!(3947),
];
pub(crate) const FIRE_SMALL_BMP: AssetRef = bitmap_asset!(3948);
pub(crate) const FIRE_HELICOPTER_BMP: AssetRef = bitmap_asset!(3949);
pub(crate) const FIRE_ALERT_BMP: AssetRef = bitmap_asset!(10004);
pub(crate) const FIRE_AFTERMATH_BMP: AssetRef = bitmap_asset!(10005);
pub(crate) const STAR_AWARD_BMP: AssetRef = bitmap_asset!(10006);
pub(crate) const TERRORIST_PORTRAIT_BMP: AssetRef = bitmap_asset!(10000);
pub(crate) const FIRE_DISPATCH_BMP: AssetRef = bitmap_asset!(10001);
pub(crate) const VIP_ARRIVAL_BMP: AssetRef = bitmap_asset!(10002);
pub(crate) const CONSTRUCTION_WAV: AssetRef = sound_asset!(7000);
pub(crate) const LOBBY_SEGMENT_WAV: AssetRef = sound_asset!(7001);
pub(crate) const NO_MONEY_WAV: AssetRef = sound_asset!(7002);
pub(crate) const DEMOLITION_WAV: AssetRef = sound_asset!(7003);
pub(crate) const PAYMENT_WAV: AssetRef = sound_asset!(10013);
pub(crate) const FIRE_ALERT_WAV: AssetRef = sound_asset!(10006);
pub(crate) const FIRE_RESPONSE_WAV: AssetRef = sound_asset!(10009);
pub(crate) const ELEVATOR_MOVE_WAV: AssetRef = sound_asset!(6000);
pub(crate) const ELEVATOR_OPEN_1_WAV: AssetRef = sound_asset!(6001);
pub(crate) const ELEVATOR_OPEN_2_WAV: AssetRef = sound_asset!(6002);
pub(crate) const CROWD_1_WAV: AssetRef = sound_asset!(8000);
#[cfg(feature = "bundled-resources")]
pub(crate) const SIMTOWER_ICON_BMP: AssetRef = icon_asset!(1);

#[cfg(feature = "bundled-resources")]
pub(crate) fn initialize_resources() -> Result<(), String> {
    Ok(())
}

#[cfg(not(feature = "bundled-resources"))]
struct OriginalResourcePack {
    assets: HashMap<(AssetKind, u16), Vec<u8>>,
}

#[cfg(not(feature = "bundled-resources"))]
static ORIGINAL_RESOURCES: OnceLock<OriginalResourcePack> = OnceLock::new();

#[cfg(not(feature = "bundled-resources"))]
pub(crate) fn initialize_resources() -> Result<(), String> {
    if ORIGINAL_RESOURCES.get().is_some() {
        return Ok(());
    }
    let path = locate_original_executable()?.ok_or_else(|| {
        "OpenTower requires an original SimTower.exe. No file was selected.".to_owned()
    })?;
    let pack = OriginalResourcePack::from_executable(&path)?;
    ORIGINAL_RESOURCES
        .set(pack)
        .map_err(|_| "original resources were initialized twice".to_owned())
}

#[cfg(not(feature = "bundled-resources"))]
impl OriginalResourcePack {
    fn from_executable(path: &PathBuf) -> Result<Self, String> {
        let bytes = fs::read(path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
        let ne = NeFile::parse(&bytes).map_err(|error| {
            format!(
                "{} is not a supported SimTower.exe: {error}",
                path.display()
            )
        })?;
        let palette = ne
            .resources()
            .iter()
            .find(|resource| {
                resource.resource_type.as_ordinal() == Some(SIMTOWER_PALETTE_TYPE)
                    && resource.id.as_ordinal() == Some(SIMTOWER_BASE_PALETTE_ID)
            })
            .map(|resource| ne.resource_data(resource))
            .ok_or_else(|| format!("{} does not contain the SimTower palette", path.display()))?;

        let mut assets = HashMap::new();
        for resource in ne.resources() {
            let Some(id) = resource.id.as_ordinal() else {
                continue;
            };
            let data = ne.resource_data(resource);
            let decoded =
                match resource.resource_type.as_ordinal() {
                    Some(RT_BITMAP) => Some((
                        AssetKind::Bitmap,
                        encode_bmp(&DibImage::decode(data).map_err(|error| {
                            format!("bitmap {id} could not be decoded: {error}")
                        })?)?,
                    )),
                    Some(RT_ICON) => Some((
                        AssetKind::Icon,
                        encode_bmp(&DibImage::decode_icon(data).map_err(|error| {
                            format!("icon {id} could not be decoded: {error}")
                        })?)?,
                    )),
                    Some(SIMTOWER_RAW_BITMAP_TYPE) => {
                        let (width, height, rgba) = decode_raw_bitmap(data, palette)?;
                        Some((AssetKind::RawBitmap, encode_rgba_bmp(width, height, &rgba)?))
                    }
                    Some(SIMTOWER_WAVE_TYPE) => Some((
                        AssetKind::Sound,
                        trim_wave(data)
                            .map_err(|error| format!("sound {id} could not be decoded: {error}"))?
                            .to_vec(),
                    )),
                    _ => None,
                };
            if let Some((kind, bytes)) = decoded {
                assets.insert((kind, id), bytes);
            }
        }
        Ok(Self { assets })
    }
}

#[cfg(not(feature = "bundled-resources"))]
fn locate_original_executable() -> Result<Option<PathBuf>, String> {
    let mut args = env::args_os().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--simtower-exe" {
            return args
                .next()
                .map(PathBuf::from)
                .map(Some)
                .ok_or_else(|| "--simtower-exe requires a file path".to_owned());
        }
        if let Some(argument) = argument.to_str() {
            if let Some(path) = argument.strip_prefix("--simtower-exe=") {
                return Ok(Some(PathBuf::from(path)));
            }
        }
    }
    if let Some(path) = env::var_os("SIMTOWER_EXE") {
        return Ok(Some(PathBuf::from(path)));
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(directory) = executable.parent() {
            let candidate = directory.join("SIMTOWER.EXE");
            if candidate.is_file() {
                return Ok(Some(candidate));
            }
        }
    }
    let candidate = PathBuf::from("SIMTOWER.EXE");
    if candidate.is_file() {
        return Ok(Some(candidate));
    }
    choose_original_executable()
}

#[cfg(all(not(feature = "bundled-resources"), target_os = "macos"))]
fn choose_original_executable() -> Result<Option<PathBuf>, String> {
    let script = "POSIX path of (choose file with prompt \"Select your original SimTower.exe\")";
    let output = Command::new("osascript")
        .args(["-e", script])
        .output()
        .map_err(|error| format!("could not launch the macOS file picker: {error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    selected_path(&output.stdout)
}

#[cfg(all(not(feature = "bundled-resources"), target_os = "windows"))]
fn choose_original_executable() -> Result<Option<PathBuf>, String> {
    let script = "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.OpenFileDialog; $d.Title='Select your original SimTower.exe'; $d.Filter='SimTower executable (SimTower.exe)|SimTower.exe|Windows executables (*.exe)|*.exe'; if($d.ShowDialog() -eq 'OK'){$d.FileName}";
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-STA", "-Command", script])
        .output()
        .map_err(|error| format!("could not launch the Windows file picker: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    selected_path(&output.stdout)
}

#[cfg(all(not(feature = "bundled-resources"), target_os = "linux"))]
fn choose_original_executable() -> Result<Option<PathBuf>, String> {
    let output = Command::new("zenity")
        .args([
            "--file-selection",
            "--title=Select your original SimTower.exe",
            "--file-filter=Windows executables | *.exe *.EXE",
        ])
        .output();
    let output = match output {
        Ok(output) => output,
        Err(_) => Command::new("kdialog")
            .args([
                "--getopenfilename",
                ".",
                "*.exe *.EXE|Windows executables",
                "--title",
                "Select your original SimTower.exe",
            ])
            .output()
            .map_err(|error| format!("neither zenity nor kdialog could be started: {error}"))?,
    };
    if !output.status.success() {
        return Ok(None);
    }
    selected_path(&output.stdout)
}

#[cfg(all(
    not(feature = "bundled-resources"),
    not(any(target_os = "macos", target_os = "windows", target_os = "linux"))
))]
fn choose_original_executable() -> Result<Option<PathBuf>, String> {
    Err("set SIMTOWER_EXE or pass --simtower-exe with the path to SimTower.exe".to_owned())
}

#[cfg(not(feature = "bundled-resources"))]
fn selected_path(stdout: &[u8]) -> Result<Option<PathBuf>, String> {
    let path = String::from_utf8(stdout.to_vec())
        .map_err(|_| "file picker returned a non-UTF-8 path".to_owned())?;
    let path = path.trim();
    Ok((!path.is_empty()).then(|| PathBuf::from(path)))
}

#[cfg(not(feature = "bundled-resources"))]
fn encode_bmp(image: &DibImage) -> Result<Vec<u8>, String> {
    encode_rgba_bmp(image.width(), image.height(), image.rgba())
}

#[cfg(not(feature = "bundled-resources"))]
fn encode_rgba_bmp(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let pixel_bytes = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "BMP dimensions overflow".to_owned())?;
    if rgba.len() != pixel_bytes as usize {
        return Err("RGBA byte count does not match dimensions".to_owned());
    }
    let file_size = 54_u32
        .checked_add(pixel_bytes)
        .ok_or_else(|| "BMP file size overflows".to_owned())?;
    let mut bmp = Vec::with_capacity(file_size as usize);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&file_size.to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&(54_u32).to_le_bytes());
    bmp.extend_from_slice(&(40_u32).to_le_bytes());
    bmp.extend_from_slice(&(width as i32).to_le_bytes());
    bmp.extend_from_slice(&(-(height as i32)).to_le_bytes());
    bmp.extend_from_slice(&(1_u16).to_le_bytes());
    bmp.extend_from_slice(&(32_u16).to_le_bytes());
    bmp.extend_from_slice(&(0_u32).to_le_bytes());
    bmp.extend_from_slice(&pixel_bytes.to_le_bytes());
    bmp.extend_from_slice(&(2_835_i32).to_le_bytes());
    bmp.extend_from_slice(&(2_835_i32).to_le_bytes());
    bmp.extend_from_slice(&(0_u32).to_le_bytes());
    bmp.extend_from_slice(&(0_u32).to_le_bytes());
    for pixel in rgba.chunks_exact(4) {
        bmp.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    Ok(bmp)
}

#[cfg(not(feature = "bundled-resources"))]
fn decode_raw_bitmap(bytes: &[u8], palette: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    const CELL_WIDTH: usize = 8;
    const HEIGHT: usize = 36;
    const PALETTE_ENTRY_BYTES: usize = 8;
    if palette.len() < 256 * PALETTE_ENTRY_BYTES {
        return Err("SimTower palette is truncated".to_owned());
    }
    let cell_count = bytes.len() / (CELL_WIDTH * HEIGHT);
    if cell_count == 0 {
        return Err("raw bitmap contains no complete cells".to_owned());
    }
    let width = cell_count * CELL_WIDTH;
    let mut rgba = vec![0_u8; width * HEIGHT * 4];
    for (index, color_index) in bytes.iter().copied().enumerate() {
        let source_x = index % CELL_WIDTH;
        let source_y = index / CELL_WIDTH;
        let destination_x = source_x + (source_y / HEIGHT) * CELL_WIDTH;
        let destination_y = source_y % HEIGHT;
        if destination_x >= width {
            continue;
        }
        let palette_offset = usize::from(color_index) * PALETTE_ENTRY_BYTES;
        let destination = (destination_y * width + destination_x) * 4;
        rgba[destination] = palette[palette_offset + 2];
        rgba[destination + 1] = palette[palette_offset + 4];
        rgba[destination + 2] = palette[palette_offset + 6];
        rgba[destination + 3] = 255;
    }
    Ok((width as u32, HEIGHT as u32, rgba))
}
