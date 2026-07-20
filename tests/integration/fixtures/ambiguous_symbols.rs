pub struct Alpha;
pub struct Beta;

impl Alpha {
    pub fn validate(&self) -> bool {
        true
    }
}

impl Beta {
    pub fn validate(&self) -> bool {
        false
    }
}
