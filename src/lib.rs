use nih_plug::prelude::*;
use std::sync::Arc;

pub mod dsp;
pub mod params;
pub mod ui;

use dsp::{Engine, Shared};
use params::FlatteryParams;

pub struct Flattery {
    params: Arc<FlatteryParams>,
    engine: Engine,
    shared: Arc<Shared>,
}

impl Default for Flattery {
    fn default() -> Self {
        let params = Arc::new(FlatteryParams::default());
        let shared = Arc::new(Shared::new());
        Self {
            params,
            engine: Engine::new(shared.clone(), 44100.0),
            shared,
        }
    }
}

impl Plugin for Flattery {
    const NAME: &'static str = "Flattery";
    const VENDOR: &'static str = "Fergler";
    const URL: &'static str = "https://github.com/Ferglerz/Flattery";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type BackgroundTask = ();
    type SysExMessage = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        ui::create(self.params.clone(), self.shared.clone())
    }

    fn initialize(
        &mut self,
        _: &AudioIOLayout,
        c: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        self.engine = Engine::new(self.shared.clone(), c.sample_rate as f64);
        context.set_latency_samples(self.engine.latency());
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        context.set_latency_samples(self.engine.latency());

        for mut frame in buffer.iter_samples() {
            let mut x = [0.0; 2];
            for (i, s) in frame.iter_mut().enumerate() {
                x[i] = if s.is_finite() { *s as f64 } else { 0.0 };
            }
            if frame.len() == 1 {
                x[1] = x[0];
            }

            let out = self.engine.tick(x[0], x[1], &self.params);

            for (i, s) in frame.iter_mut().enumerate() {
                *s = if i == 0 { out.0 as f32 } else { out.1 as f32 };
            }
        }

        if self.engine.latency() > 0 {
            ProcessStatus::Tail(self.engine.latency() * 2)
        } else {
            ProcessStatus::Normal
        }
    }
}

impl ClapPlugin for Flattery {
    const CLAP_ID: &'static str = "com.fergler.flattery";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Frequency-domain leveling and spectral shaping plugin");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Filter,
        ClapFeature::Utility,
        ClapFeature::Equalizer,
    ];
}

impl Vst3Plugin for Flattery {
    const VST3_CLASS_ID: [u8; 16] = *b"FerglerFlattery_";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Fx,
        Vst3SubCategory::Eq,
        Vst3SubCategory::Restoration,
    ];
}

nih_export_clap!(Flattery);
nih_export_vst3!(Flattery);
