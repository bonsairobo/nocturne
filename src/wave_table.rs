use std::f32;
use std::sync::Once;

const WAVE_TABLE_SIZE: usize = 1 << 16;

fn table_sample_conversion_factor(sample_hz: f32) -> f32 {
    WAVE_TABLE_SIZE as f32 / sample_hz
}

static INIT: Once = Once::new();

static mut SQUARE_WAVE: [f32; WAVE_TABLE_SIZE] = [0.0; WAVE_TABLE_SIZE];
static mut SAWTOOTH_WAVE: [f32; WAVE_TABLE_SIZE] = [0.0; WAVE_TABLE_SIZE];
static mut TRIANGLE_WAVE: [f32; WAVE_TABLE_SIZE] = [0.0; WAVE_TABLE_SIZE];
static mut SINE_WAVE: [f32; WAVE_TABLE_SIZE] = [0.0; WAVE_TABLE_SIZE];

// Wave functions must be defined on the domain [0.0, 1.0], preferably with a codomain of [-1.0,
// 1.0].

fn init_wave<F>(wave_fn: F, table: &mut [f32])
where
    F: Fn(f32) -> f32,
{
    for (i, item) in table.iter_mut().enumerate() {
        *item = wave_fn(i as f32 / WAVE_TABLE_SIZE as f32);
    }
}

fn square_wave(t: f32) -> f32 {
    if t > 0.5 {
        1.0
    } else {
        -1.0
    }
}

fn sawtooth_wave(t: f32) -> f32 {
    2.0 * (t % 1.0) - 1.0
}

fn triangle_wave(t: f32) -> f32 {
    2.0 * sawtooth_wave(t).abs() - 1.0
}

fn sine_wave(t: f32) -> f32 {
    (2.0 * f32::consts::PI * t).sin()
}

pub fn get_square_wave() -> [f32; WAVE_TABLE_SIZE] {
    unsafe {
        INIT.call_once(|| init_wave(square_wave, &mut SQUARE_WAVE[..]));
        SQUARE_WAVE
    }
}

pub fn get_sawtooth_wave() -> [f32; WAVE_TABLE_SIZE] {
    unsafe {
        INIT.call_once(|| init_wave(sawtooth_wave, &mut SAWTOOTH_WAVE[..]));
        SAWTOOTH_WAVE
    }
}

pub fn get_triangle_wave() -> [f32; WAVE_TABLE_SIZE] {
    unsafe {
        INIT.call_once(|| init_wave(triangle_wave, &mut TRIANGLE_WAVE[..]));
        TRIANGLE_WAVE
    }
}

pub fn get_sine_wave() -> [f32; WAVE_TABLE_SIZE] {
    unsafe {
        INIT.call_once(|| init_wave(sine_wave, &mut SINE_WAVE[..]));
        SINE_WAVE
    }
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
