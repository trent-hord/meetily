use anyhow;
use tracing::info;
#[derive(Clone, Copy, Debug, Default)]
pub enum VadSensitivity {
    Low,
    #[default]
    Medium,
    High,
}

impl VadSensitivity {
    pub fn min_speech_ratio(&self) -> f32 {
        match self {
            VadSensitivity::Low => 0.01,    // 1% of frames must be speech
            VadSensitivity::Medium => 0.05, // 5% of frames must be speech
            VadSensitivity::High => 0.2,    // 20% of frames must be speech
        }
    }
}

pub enum VadEngineEnum {
    WebRtc,
}

pub trait VadEngine: Send {
    fn is_voice_segment(&mut self, audio_chunk: &[f32]) -> anyhow::Result<bool>;
    fn set_sensitivity(&mut self, sensitivity: VadSensitivity);
    fn get_min_speech_ratio(&self) -> f32;
}

#[derive(Default)]
pub struct WebRtcVad {
    vad: webrtc_vad::Vad,
    sensitivity: VadSensitivity,
}

impl WebRtcVad {
    pub fn new() -> Self {
        let vad = webrtc_vad::Vad::new();
        Self {
            vad,
            sensitivity: VadSensitivity::Medium,
        }
    }
}

impl VadEngine for WebRtcVad {
    fn is_voice_segment(&mut self, audio_chunk: &[f32]) -> anyhow::Result<bool> {
        // Convert f32 to i16
        let i16_chunk: Vec<i16> = audio_chunk.iter().map(|&x| (x * 32767.0) as i16).collect();

        // Set VAD mode based on sensitivity
        let mode = match self.sensitivity {
            VadSensitivity::Low => webrtc_vad::VadMode::Quality,
            VadSensitivity::Medium => webrtc_vad::VadMode::Aggressive,
            VadSensitivity::High => webrtc_vad::VadMode::VeryAggressive,
        };
        self.vad.set_mode(mode);

        let result = self
            .vad
            .is_voice_segment(&i16_chunk)
            .map_err(|e| anyhow::anyhow!("WebRTC VAD error: {:?}", e))?;

        Ok(result)
    }
    }

    fn set_sensitivity(&mut self, sensitivity: VadSensitivity) {
        self.sensitivity = sensitivity;
    }

    fn get_min_speech_ratio(&self) -> f32 {
        self.sensitivity.min_speech_ratio()
    }
}

pub async fn create_vad_engine(engine: VadEngineEnum) -> anyhow::Result<Box<dyn VadEngine>> {
    match engine {
        VadEngineEnum::WebRtc => Ok(Box::new(WebRtcVad::new())),
    }
}

unsafe impl Send for WebRtcVad {}
