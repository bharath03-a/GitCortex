pub struct Worker {
    pub name: String,
}

impl Processor for Worker {
    fn process(&self) -> String {
        format!("processing: {}", self.name)
    }

    fn validate(&self) -> bool {
        !self.name.is_empty()
    }
}
