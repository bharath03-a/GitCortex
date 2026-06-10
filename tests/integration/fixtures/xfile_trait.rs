pub trait Processor {
    fn process(&self) -> String;
    fn validate(&self) -> bool;
}
