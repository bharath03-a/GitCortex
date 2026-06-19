use crate::xfile_callee::compute_value;

pub fn run() -> i32 {
    let result = compute_value();
    result * 2
}

pub fn run_with_branch(flag: bool) -> i32 {
    if flag {
        compute_value()
    } else {
        0
    }
}
