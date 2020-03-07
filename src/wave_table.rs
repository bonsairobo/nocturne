use once_cell::sync::OnceCell;
use std::f32;

const WAVE_TABLE_SIZE: usize = 1 << 16;

fn table_sample_conversion_factor(sample_hz: f32) -> f32 {
    WAVE_TABLE_SIZE as f32 / sample_hz
}

// Wave functions must be defined on the domain [0.0, 1.0], preferably with a codomain of [-1.0,
// 1.0].

fn init_wave<F>(wave_fn: F) -> [f32; WAVE_TABLE_SIZE]
where
    F: Fn(f32) -> f32,
{
    let mut table = [0.0; WAVE_TABLE_SIZE];
    for (i, item) in table.iter_mut().enumerate() {
        *item = wave_fn(i as f32 / WAVE_TABLE_SIZE as f32);
    }

    table
}

fn square_wave_fn(t: f32) -> f32 {
    if t > 0.5 {
        1.0
    } else {
        -1.0
    }
}

fn sawtooth_wave_fn(t: f32) -> f32 {
    2.0 * (t % 1.0) - 1.0
}

fn triangle_wave_fn(t: f32) -> f32 {
    2.0 * sawtooth_wave_fn(t).abs() - 1.0
}

fn sine_wave_fn(t: f32) -> f32 {
    (2.0 * f32::consts::PI * t).sin()
}

pub type Wave = &'static [f32];

pub fn square_wave() -> Wave {
    static SQUARE_WAVE: OnceCell<[f32; WAVE_TABLE_SIZE]> = OnceCell::new();

    SQUARE_WAVE.get_or_init(|| init_wave(square_wave_fn))
}

pub fn sawtooth_wave() -> Wave {
    static SAWTOOTH_WAVE: OnceCell<[f32; WAVE_TABLE_SIZE]> = OnceCell::new();

    SAWTOOTH_WAVE.get_or_init(|| init_wave(sawtooth_wave_fn))
}

pub fn triangle_wave() -> Wave {
    static TRIANGLE_WAVE: OnceCell<[f32; WAVE_TABLE_SIZE]> = OnceCell::new();

    TRIANGLE_WAVE.get_or_init(|| init_wave(triangle_wave_fn))
}

pub fn sine_wave() -> Wave {
    static SINE_WAVE: OnceCell<[f32; WAVE_TABLE_SIZE]> = OnceCell::new();

    SINE_WAVE.get_or_init(|| init_wave(sine_wave_fn))
}

pub struct WaveTableIndex {
    index: f32,
    indices_per_sample: f32,
}

impl WaveTableIndex {
    pub fn new(start_index: f32, indices_per_sample: f32) -> Self {
        WaveTableIndex {
            index: start_index,
            indices_per_sample,
        }
    }

    pub fn from_hz(sample_hz: f32, hz: f32) -> Self {
        Self::new(0.0, hz * table_sample_conversion_factor(sample_hz))
    }

    pub fn sample_table(&mut self, table: &[f32]) -> f32 {
        let sample = table[self.index.floor() as usize];
        self.index = (self.index + self.indices_per_sample) % table.len() as f32;

        sample
    }
}
