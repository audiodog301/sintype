use crate::generator::Generator;

// TODO: make it not go out of tune the higher it gets
#[derive(Clone)]
pub struct SawGenerator {
    frequency: f32,
    count: i32,
    val: f32,
}

impl SawGenerator {
    pub fn new(frequency: f32) -> Self {
        Self {
            frequency: frequency,
            count: 0,
            val: 0.0,
        }
    }
}

impl Generator for SawGenerator {
    fn next_sample(&mut self, sample_rate: f32) -> f32 {
        if self.count >= (sample_rate / self.frequency) as i32 {
            self.count = 0;
            self.val = 1.0;
        } else {
            self.count += 1;
            self.val -= 1.0 / (sample_rate / self.frequency);
        }

        self.val - 0.5
    }

    fn input_control(&mut self, _inputs: Vec<f32>) {}
}
