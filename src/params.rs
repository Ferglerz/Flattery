use nih_plug::prelude::*;

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum FftSize {
    #[name = "128"]
    Fft128,
    #[name = "256"]
    Fft256,
    #[name = "512"]
    Fft512,
    #[name = "1024"]
    Fft1024,
    #[name = "2048"]
    Fft2048,
    #[name = "4096"]
    Fft4096,
    #[name = "8192"]
    Fft8192,
}

impl FftSize {
    pub fn size(&self) -> usize {
        match self {
            Self::Fft128 => 128,
            Self::Fft256 => 256,
            Self::Fft512 => 512,
            Self::Fft1024 => 1024,
            Self::Fft2048 => 2048,
            Self::Fft4096 => 4096,
            Self::Fft8192 => 8192,
        }
    }
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum ProcessDomain {
    #[name = "L/R"]
    LR,
    #[name = "M/S"]
    MS,
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum DifferenceMode {
    #[name = "Reduce"]
    Reduce,
    #[name = "Amplify"]
    Amplify,
}

use crate::strength::StrengthNode;
use nih_plug_vizia::ViziaState;
use std::sync::{Arc, Mutex};

#[derive(Params)]
pub struct FlatteryParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<ViziaState>,

    #[persist = "boost-nodes"]
    pub boost_nodes: Arc<Mutex<Vec<StrengthNode>>>,

    #[persist = "cut-nodes"]
    pub cut_nodes: Arc<Mutex<Vec<StrengthNode>>>,

    #[id = "fft_size"]
    pub fft_size: EnumParam<FftSize>,

    #[id = "strength_boost"]
    pub strength_boost: FloatParam,

    #[id = "strength_cut"]
    pub strength_cut: FloatParam,

    #[id = "max_boost_db"]
    pub max_boost_db: FloatParam,

    #[id = "max_cut_db"]
    pub max_cut_db: FloatParam,

    #[id = "output_gain_db"]
    pub output_gain_db: FloatParam,

    #[id = "ms_mode"]
    pub ms_mode: EnumParam<ProcessDomain>,

    #[id = "stereo_link"]
    pub stereo_link: FloatParam,

    #[id = "neighbor_radius"]
    pub neighbor_radius: IntParam,

    #[id = "amplify_mode"]
    pub amplify_mode: EnumParam<DifferenceMode>,

    #[id = "attack_ms"]
    pub attack_ms: FloatParam,

    #[id = "release_ms"]
    pub release_ms: FloatParam,

    #[id = "input_rms_ms"]
    pub input_rms_ms: FloatParam,

    #[id = "min_operate_db"]
    pub min_operate_db: FloatParam,

    #[id = "max_operate_db"]
    pub max_operate_db: FloatParam,

    #[id = "low_cut_hz"]
    pub low_cut_hz: FloatParam,

    #[id = "high_cut_hz"]
    pub high_cut_hz: FloatParam,

    #[id = "tilt"]
    pub tilt: FloatParam,

    #[id = "tilt_freq_hz"]
    pub tilt_freq_hz: FloatParam,

    #[id = "bypass"]
    pub bypass: BoolParam,
}

impl Default for FlatteryParams {
    fn default() -> Self {
        Self {
            editor_state: ViziaState::new(|| (1040, 660)),
            boost_nodes: Arc::new(Mutex::new(Vec::new())),
            cut_nodes: Arc::new(Mutex::new(Vec::new())),
            fft_size: EnumParam::new("FFT Size", FftSize::Fft512),

            strength_boost: FloatParam::new(
                "Strength Boost",
                35.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 200.0,
                },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            strength_cut: FloatParam::new(
                "Strength Cut",
                35.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 200.0,
                },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            max_boost_db: FloatParam::new(
                "Max Boost",
                12.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 48.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            max_cut_db: FloatParam::new(
                "Max Cut",
                12.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 48.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            output_gain_db: FloatParam::new(
                "Output Gain",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            ms_mode: EnumParam::new("Process Domain", ProcessDomain::MS),

            stereo_link: FloatParam::new(
                "Stereo Link",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 100.0,
                },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            neighbor_radius: IntParam::new(
                "Neighbor Radius",
                1,
                IntRange::Linear { min: 1, max: 12 },
            ),

            amplify_mode: EnumParam::new("Difference Mode", DifferenceMode::Reduce),

            attack_ms: FloatParam::new(
                "Attack",
                2.0,
                FloatRange::Skewed {
                    min: 0.1,
                    max: 200.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(Arc::new(|v| {
                if v < 10.0 {
                    format!("{:.1}", v)
                } else {
                    format!("{:.0}", v)
                }
            })),

            release_ms: FloatParam::new(
                "Release",
                30.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 2000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(Arc::new(|v| {
                if v < 10.0 {
                    format!("{:.1}", v)
                } else {
                    format!("{:.0}", v)
                }
            })),

            input_rms_ms: FloatParam::new(
                "Input RMS",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 10.0,
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            min_operate_db: FloatParam::new(
                "Operate Min",
                -120.0,
                FloatRange::Linear {
                    min: -120.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            max_operate_db: FloatParam::new(
                "Operate Max",
                0.0,
                FloatRange::Linear {
                    min: -120.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            low_cut_hz: FloatParam::new(
                "Low Cut",
                20.0,
                FloatRange::Skewed {
                    min: 10.0,
                    max: 20000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            high_cut_hz: FloatParam::new(
                "High Cut",
                20000.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            tilt: FloatParam::new(
                "Tilt",
                0.0,
                FloatRange::Linear {
                    min: -100.0,
                    max: 100.0,
                },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            tilt_freq_hz: FloatParam::new(
                "Tilt Freq",
                1800.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            bypass: BoolParam::new("Bypass", false),
        }
    }
}
