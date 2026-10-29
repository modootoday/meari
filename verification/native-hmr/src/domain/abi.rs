//! Fixed C ABI. No Rust references, closures, allocators or vtables cross it.
pub const ABI: u64 = 0x5255_495f_3030_3031;
pub const SCHEMA: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Contract {
    pub abi: u64,
    pub schema: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Input {
    pub counter: i64,
}

#[repr(C)]
pub struct Frame {
    pub generation: u32,
    pub heading: *const u8,
    pub heading_len: usize,
    pub button: *const u8,
    pub button_len: usize,
    pub computed: i64,
}

pub type ContractFn = unsafe extern "C" fn() -> Contract;
pub type RenderFn = unsafe extern "C" fn(Input) -> Frame;

pub fn compatible(contract: Contract) -> bool {
    contract.abi == ABI && contract.schema == SCHEMA
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incompatible_abi_or_state_schema_is_rejected() {
        assert!(compatible(Contract {
            abi: ABI,
            schema: SCHEMA
        }));
        assert!(!compatible(Contract {
            abi: ABI + 1,
            schema: SCHEMA
        }));
        assert!(!compatible(Contract {
            abi: ABI,
            schema: SCHEMA + 1
        }));
    }
}
