mod characters;
mod layout;
mod state;

use std::{borrow::Cow, collections::BTreeMap};

use enumset::{EnumSet, EnumSetType};
use kime_engine_backend::{InputEngineBackend, Key, KeyCode};
use serde::{Deserialize, Serialize};

pub use layout::{Layout, LayoutError, LAYOUT_FORMAT_VERSION};
pub use state::HangulEngine;

#[derive(Hash, Serialize, Deserialize, Debug, EnumSetType)]
#[enumset(serialize_repr = "list")]
pub enum Addon {
    ComposeChoseongSsang,
    ComposeJungseongSsang,
    ComposeJongseongSsang,
    DecomposeChoseongSsang,
    DecomposeJungseongSsang,
    DecomposeJongseongSsang,

    /// ㅏ + ㄱ = 가
    /// ㄱ + $ㄱ + ㅏ = 각
    FlexibleComposeOrder,

    /// 안 + ㅣ = 아니
    TreatJongseongAsChoseong,
    /// 읅 + ㄱ = 을ㄲ
    TreatJongseongAsChoseongCompose,

    /// 세벌식 순아래: 서로 다른 자음을 붙여 눌러 된소리 입력 (ㄱ+ㅇ=ㄲ 등)
    ComposeChoseongSunahrae,
    /// 세벌식 순아래: 조합용ㅗ(ㅚ)/조합용ㅜ(ㅟ) 키의 추가 이중모음 조합
    ComposeJungseongSunahrae,
    /// 세벌식 순아래: `/`와 `[` 키가 초성 유무에 따라 문자 조합용/원래 기호로 갈라짐
    SunahraeContextKeys,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PreeditJohabLevel {
    /// Always use johab encoding
    Always,
    /// Use johab encoding when it's needed
    Needed,
    /// Never use johab encoding
    Never,
}

impl Default for PreeditJohabLevel {
    fn default() -> Self {
        PreeditJohabLevel::Needed
    }
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct HangulConfig {
    pub layout: String,
    pub word_commit: bool,
    pub preedit_johab: PreeditJohabLevel,
    pub addons: BTreeMap<String, EnumSet<Addon>>,
}

impl Default for HangulConfig {
    fn default() -> Self {
        Self {
            layout: "dubeolsik".into(),
            word_commit: false,
            preedit_johab: PreeditJohabLevel::default(),
            addons: vec![
                ("all".into(), Addon::ComposeChoseongSsang.into()),
                ("dubeolsik".into(), Addon::TreatJongseongAsChoseong.into()),
                (
                    "sebeolsik-3sin-p2".into(),
                    Addon::ComposeJongseongSsang.into(),
                ),
                (
                    "sebeolsik-sunahrae".into(),
                    Addon::ComposeChoseongSunahrae
                        | Addon::ComposeJungseongSunahrae
                        | Addon::SunahraeContextKeys,
                ),
            ]
            .into_iter()
            .collect(),
        }
    }
}

pub const BUILTIN_LAYOUTS: &'static [(&'static str, &'static str)] = &[
    ("dubeolsik", include_str!("../data/dubeolsik.yaml")),
    (
        "sebeolsik-3-90",
        include_str!("../data/sebeolsik-3-90.yaml"),
    ),
    (
        "sebeolsik-3-91",
        include_str!("../data/sebeolsik-3-91.yaml"),
    ),
    (
        "sebeolsik-3sin-1995",
        include_str!("../data/sebeolsik-3sin-1995.yaml"),
    ),
    (
        "sebeolsik-3sin-p2",
        include_str!("../data/sebeolsik-3sin-p2.yaml"),
    ),
    (
        "sebeolsik-sunahrae",
        include_str!("../data/sebeolsik-sunahrae.yaml"),
    ),
];

pub struct HangulData {
    layout: Layout,
    addons: EnumSet<Addon>,
    preedit_johab: PreeditJohabLevel,
    word_commit: bool,
}

impl Default for HangulData {
    fn default() -> Self {
        Self::new(&HangulConfig::default(), builtin_layouts())
    }
}

impl HangulData {
    #[cfg(unix)]
    pub fn from_config_with_dir(config: &HangulConfig, dir: &xdg::BaseDirectories) -> Self {
        let custom_layouts = dir
            .list_config_files("layouts")
            .into_iter()
            .filter_map(|path| {
                let name = path.file_stem()?.to_str()?.to_string();

                let content = match std::fs::read_to_string(&path) {
                    Ok(content) => content,
                    Err(err) => {
                        log::error!("Can't read layout file {}: {}", path.display(), err);
                        return None;
                    }
                };

                match Layout::load_from(&content) {
                    Ok(layout) => Some((name.into(), layout)),
                    Err(err) => {
                        log::error!("Can't load layout file {}: {}", path.display(), err);
                        None
                    }
                }
            });

        Self::new(config, custom_layouts.chain(builtin_layouts()))
    }

    pub fn new(
        config: &HangulConfig,
        mut layouts: impl Iterator<Item = (Cow<'static, str>, Layout)>,
    ) -> Self {
        Self {
            layout: layouts
                .find_map(|(name, layout)| {
                    if name == config.layout {
                        Some(layout)
                    } else {
                        None
                    }
                })
                .unwrap_or_default(),
            addons: config.addons.get("all").copied().unwrap_or_default().union(
                config
                    .addons
                    .get(&config.layout)
                    .copied()
                    .unwrap_or_default(),
            ),
            preedit_johab: config.preedit_johab,
            word_commit: config.word_commit,
        }
    }

    pub const fn preedit_johab(&self) -> PreeditJohabLevel {
        self.preedit_johab
    }

    pub const fn word_commit(&self) -> bool {
        self.word_commit
    }
}

impl InputEngineBackend for HangulEngine {
    type ConfigData = HangulData;

    fn press_key(&mut self, config: &HangulData, key: Key, commit_buf: &mut String) -> bool {
        // 세벌식 순아래: `[` 뒤에 예약해둔 종성 조합을 여기서 소비한다. 391의 Shift 슬롯과는
        // 독립된 별도 테이블(sunahrae_bracket_jongseong)을 쓰기 때문에, 진짜 Shift+숫자는
        // 겹받침에 안 뺏기고 일반 기호(!@#$% 등)로 자유롭게 쓸 수 있다.
        if self.take_pending_shift() {
            if let Some(kv) = sunahrae_bracket_jongseong(key.code) {
                return self.key(kv, config.addons, commit_buf);
            }
            // 매핑에 없는 키가 오면 예약만 조용히 소비하고, 그 키 자체는 평소대로 처리한다.
        }

        if key.code == KeyCode::Backspace {
            return self.backspace(config.addons, commit_buf);
        }

        // 세벌식 순아래: `/`(조합용ㅗ)와 `[`(종성조합글쇠)는 초성이 없으면 원래 기호를,
        // 초성이 있으면 한글 조합 동작을 한다 (날개셋 원 배열의 검증된 규칙).
        if config.addons.contains(Addon::SunahraeContextKeys) && key.state.is_empty() {
            match key.code {
                KeyCode::Slash if !self.has_choseong() => {
                    self.clear_preedit(commit_buf);
                    commit_buf.push('/');
                    return true;
                }
                KeyCode::OpenBracket if !self.has_choseong() => {
                    self.clear_preedit(commit_buf);
                    commit_buf.push('[');
                    return true;
                }
                KeyCode::OpenBracket => {
                    // 초성이 있는 채로 `[`가 눌리면: 다음 키의 종성 조합값을 예약만 하고
                    // 화면에는 아무것도 찍지 않는다.
                    self.set_pending_shift();
                    return true;
                }
                _ => {}
            }
        }

        if let Some(kv) = config.layout.lookup_kv(key) {
            self.key(kv, config.addons, commit_buf)
        } else {
            false
        }
    }

    #[inline]
    fn clear_preedit(&mut self, commit_buf: &mut String) {
        self.clear_preedit(commit_buf);
    }

    #[inline]
    fn reset(&mut self) {
        self.reset();
    }

    #[inline]
    fn has_preedit(&self) -> bool {
        self.has_preedit()
    }

    fn preedit_str(&self, buf: &mut String) {
        self.preedit_str(buf);
    }
}

/// 세벌식 순아래: `[`(종성조합글쇠) 다음에 눌리는 후보 키 → 겹받침/특수 종성 값.
/// 391의 Shift 슬롯을 그대로 재사용하지 않는 독립 테이블이라, 실제 Shift+숫자/자모 키는
/// 이 표와 무관하게 다른 값(예: 일반 기호)으로 자유롭게 쓸 수 있다.
/// (원 배열 문서 기준: https://text.youknowone.org/post/106848470561/3final-noshift)
fn sunahrae_bracket_jongseong(code: KeyCode) -> Option<characters::KeyValue> {
    use characters::{Jongseong::*, KeyValue};

    let jong = match code {
        // 자음 줄: ㅎ+[=ㄲ, ㅆ+[=ㄺ, ㅂ+[=ㅈ, ㅅ+[=ㅍ, ㄹ+[=ㅌ, ㅇ+[=ㄷ, ㄴ+[=ㄶ, ㅁ+[=ㅊ, ㄱ+[=ㅄ
        KeyCode::One => SsangGiyeok,
        KeyCode::Two => RieulGiyeok,
        KeyCode::Three => Jieut,
        KeyCode::Q => Pieup,
        KeyCode::W => Tieut,
        KeyCode::A => Digeut,
        KeyCode::S => NieunHieuh,
        KeyCode::Z => Chieut,
        KeyCode::X => BieupSiot,
        // 모음 줄: ㅛ+[=ㄿ, ㅠ+[=ㄾ, ㅕ+[=ㄵ, ㅐ+[=ㅀ, ㅓ+[=ㄽ, ㅣ+[=ㄼ, ㅏ+[=ㄻ, ㅔ+[=ㅋ, ㅗ+[=ㄳ
        KeyCode::Four => RieulPieup,
        KeyCode::Five => RieulTieut,
        KeyCode::E => NieunJieut,
        KeyCode::R => RieulHieuh,
        KeyCode::T => RieulSiot,
        KeyCode::D => RieulBieup,
        KeyCode::F => RieulMieum,
        KeyCode::C => Kieuk,
        KeyCode::V => GiyeokSiot,
        _ => return None,
    };

    Some(KeyValue::Jongseong { jong })
}

pub fn builtin_layouts() -> impl Iterator<Item = (Cow<'static, str>, Layout)> {
    BUILTIN_LAYOUTS
        .iter()
        .copied()
        .filter_map(|(name, layout)| Layout::load_from(layout).ok().map(|l| (name.into(), l)))
}
