/*
 * Created on Wed Oct 28 2020:13:50:19
 * Created by Ratnadeep Bhattacharya
 */

// alias for macros
// extern crate self as engine;

pub mod config;
mod dpdk;
mod ffi;
mod macros;
// pub mod metrics;
pub mod net;
mod runtime;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
