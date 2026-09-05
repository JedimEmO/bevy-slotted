use mlua::{Lua, LuaOptions, StdLib, Value};

fn present(lua: &Lua, label: &str) {
    let names = ["io","os","package","require","dofile","loadfile","load","loadstring","debug","collectgarbage","getfenv","setfenv","newproxy","print","string","table","math","bit32","coroutine","buffer","vector","utf8"];
    let p: Vec<&str> = names.into_iter().filter(|n| lua.globals().get::<Value>(*n).map(|v| v != Value::Nil).unwrap_or(false)).collect();
    println!("{label:26} present: {p:?}");
}

fn main() {
    eprintln!("step1");
    let a = Lua::new();
    present(&a, "Lua::new()");
    eprintln!("step2");
    let c = Lua::new_with(StdLib::TABLE | StdLib::STRING | StdLib::MATH, LuaOptions::new()).unwrap();
    present(&c, "TABLE|STRING|MATH");
    eprintln!("step3");
    let d = Lua::new();
    for n in ["os","debug","getfenv","setfenv","newproxy","coroutine","collectgarbage"] { d.globals().set(n, Value::Nil).unwrap(); }
    d.sandbox(true).unwrap();
    present(&d, "removed + sandbox(true)");
    println!("  write after sandbox -> {:?}", d.globals().set("math", Value::Nil).map_err(|e| e.to_string()));
    println!("  math after that: {:?}", d.globals().get::<Value>("math").map(|v| v.type_name()));
    println!("  script writing a global: {:?}", d.load("zzz = 1").exec().map_err(|e| e.to_string()));
    eprintln!("step4");
    println!("  memory_limit: {:?}", d.set_memory_limit(1024*1024));
    eprintln!("done");
}
