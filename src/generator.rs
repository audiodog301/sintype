use std::cell::RefCell;
use std::collections::HashSet;

pub trait Generator: Send {
    fn next_sample(&mut self, sample_rate: f32) -> f32;
    fn input_control(&mut self, inputs: Vec<f32>);
}

pub struct GeneratorWrapper {
    id: String,
    pub dependencies: HashSet<String>,
    pub generator: Box<RefCell<dyn Generator>>,
}

impl GeneratorWrapper {
    pub fn new(id: &str, generator: Box<RefCell<dyn Generator>>) -> Self {
        GeneratorWrapper {
            id: id.to_string(),
            dependencies: HashSet::new(),
            generator,
        }
    }

    pub fn get_id(&self) -> String {
        self.id.clone()
    }
}
