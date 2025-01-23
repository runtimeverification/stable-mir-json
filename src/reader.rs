// Reads the json format written by printer::emit_smir back into data structures from stable_mir.
// This does not fully reproduce the stable_mir held inside the compiler during a compilation,
// therefore we have to use a custom data structure to hold the result (but with components
// from the stable_mir crate)

use std::{collections::HashMap,fs::File,io,iter::Iterator,vec::Vec,str,};

// this codebase needs to use external imports...
extern crate serde;
extern crate serde_json;
use serde::{Deserialize, Deserializer};

extern crate stable_mir;
use stable_mir::{ 
    CrateItem,
    CrateDef,
    ItemKind,
};

