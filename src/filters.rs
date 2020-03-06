/// Low-pass filter, AKA exponential smoothing. The discretized version of an RC low-pass filter.
pub struct ExponentialSmoothing {
    smoothed_value: f32,
    factor: f32,
}

impl ExponentialSmoothing {
    pub fn new(factor: f32) -> Self {
        ExponentialSmoothing {
            smoothed_value: 0.0,
            factor,
        }
    }

    pub fn apply(&mut self, sample: f32) -> f32 {
        self.smoothed_value = self.factor * sample + (1.0 - self.factor) * self.smoothed_value;

        self.smoothed_value
    }
}
