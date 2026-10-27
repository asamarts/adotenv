use std::{
    alloc::{GlobalAlloc, Layout, System},
    collections::BTreeMap,
    env, fs,
    hint::black_box,
    io::Cursor,
    sync::atomic::{AtomicUsize, Ordering},
};

use adotenv::{
    load::{load_files, load_files_with_options, load_values, LoadOptions},
    Dialect,
};

struct Counting;

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static DEALLOCATED: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let current = ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(current, Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn status_kb(field: &str) -> usize {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix(field)
                    .and_then(|value| value.split_whitespace().next())
                    .and_then(|value| value.parse().ok())
            })
        })
        .unwrap_or(0)
}

fn rss_kb() -> usize {
    status_kb("VmRSS:")
}

fn main() {
    let mode = env::args().nth(1).expect("mode");
    let iterations: usize = env::args().nth(2).expect("iterations").parse().unwrap();
    let path = env::args().nth(3).expect("fixture");
    let source = fs::read_to_string(&path).unwrap();

    if mode.starts_with("rss_") {
        match mode.as_str() {
            "rss_noop" => {
                println!(
                    "{} rss_kb={} hwm_kb={} len={}",
                    mode,
                    rss_kb(),
                    status_kb("VmHWM:"),
                    source.len()
                );
            }
            "rss_adotenv_parse" => {
                let doc = adotenv::parse_strict(&source).unwrap();
                println!(
                    "{} rss_kb={} hwm_kb={} len={}",
                    mode,
                    rss_kb(),
                    status_kb("VmHWM:"),
                    doc.effective_entries().count()
                );
            }
            "rss_adotenv_parse_values" => {
                let values = adotenv::parse_values(&source).unwrap();
                println!(
                    "{} rss_kb={} hwm_kb={} len={}",
                    mode,
                    rss_kb(),
                    status_kb("VmHWM:"),
                    values.len()
                );
            }
            "rss_adotenv_map" => {
                let doc = adotenv::parse_strict(&source).unwrap();
                let map: BTreeMap<String, String> = doc
                    .effective_entries()
                    .map(|entry| (entry.key().to_string(), entry.value().to_string()))
                    .collect();
                drop(doc);
                println!(
                    "{} rss_kb={} hwm_kb={} len={}",
                    mode,
                    rss_kb(),
                    status_kb("VmHWM:"),
                    map.len()
                );
            }
            "rss_dotenvy_map" => {
                let map: BTreeMap<String, String> =
                    dotenvy::from_read_iter(Cursor::new(source.as_bytes()))
                        .map(|item| {
                            let (key, value) = item.unwrap();
                            (key, value)
                        })
                        .collect();
                println!(
                    "{} rss_kb={} hwm_kb={} len={}",
                    mode,
                    rss_kb(),
                    status_kb("VmHWM:"),
                    map.len()
                );
            }
            "rss_dotenv_compat" => {
                let map = dotenv_compat::parse(source.as_bytes());
                println!(
                    "{} rss_kb={} hwm_kb={} len={}",
                    mode,
                    rss_kb(),
                    status_kb("VmHWM:"),
                    map.len()
                );
            }
            "rss_dotenv_ng" => {
                let map = dotenv_ng_core::EnvLoader::with_reader(Cursor::new(source.as_bytes()))
                    .sequence(dotenv_ng_core::EnvSequence::InputOnly)
                    .load()
                    .unwrap();
                println!(
                    "{} rss_kb={} hwm_kb={} len={}",
                    mode,
                    rss_kb(),
                    status_kb("VmHWM:"),
                    map.len()
                );
            }
            "rss_dotenv_parser" => {
                let map = dotenv_parser::parse_dotenv(&source).unwrap();
                println!(
                    "{} rss_kb={} hwm_kb={} len={}",
                    mode,
                    rss_kb(),
                    status_kb("VmHWM:"),
                    map.len()
                );
            }
            "rss_adotenv_values" => {
                let values = load_values([path.as_str()], Dialect::PORTABLE).unwrap();
                println!(
                    "{} rss_kb={} hwm_kb={} len={}",
                    mode,
                    rss_kb(),
                    status_kb("VmHWM:"),
                    values.len()
                );
            }
            "rss_adotenv_load_default" => {
                let loaded = load_files([path.as_str()], Dialect::PORTABLE).unwrap();
                println!(
                    "{} rss_kb={} hwm_kb={} len={}",
                    mode,
                    rss_kb(),
                    status_kb("VmHWM:"),
                    loaded.len()
                );
            }
            "rss_adotenv_load_full" => {
                let loaded =
                    load_files_with_options([path.as_str()], LoadOptions::full(Dialect::PORTABLE))
                        .unwrap();
                println!(
                    "{} rss_kb={} hwm_kb={} len={}",
                    mode,
                    rss_kb(),
                    status_kb("VmHWM:"),
                    loaded.len()
                );
            }
            other => panic!("unknown mode {other}"),
        }
        return;
    }

    let base_live = ALLOCATED.load(Ordering::Relaxed) - DEALLOCATED.load(Ordering::Relaxed);
    let base_peak = PEAK.load(Ordering::Relaxed);

    let mut total = 0usize;
    for _ in 0..iterations {
        total += match mode.as_str() {
            "noop" => black_box(source.len()),
            "adotenv_parse" => {
                let doc = adotenv::parse_strict(black_box(&source)).unwrap();
                doc.effective_entries().count()
            }
            "adotenv_parse_values" => {
                let values = adotenv::parse_values(black_box(&source)).unwrap();
                let len = values.len();
                black_box(values);
                len
            }
            "adotenv_map" => {
                let doc = adotenv::parse_strict(black_box(&source)).unwrap();
                let map: BTreeMap<String, String> = doc
                    .effective_entries()
                    .map(|entry| (entry.key().to_string(), entry.value().to_string()))
                    .collect();
                let len = map.len();
                black_box(map);
                len
            }
            "dotenvy_map" => {
                let map: BTreeMap<String, String> =
                    dotenvy::from_read_iter(Cursor::new(black_box(source.as_bytes())))
                        .map(|item| {
                            let (key, value) = item.unwrap();
                            (key, value)
                        })
                        .collect();
                let len = map.len();
                black_box(map);
                len
            }
            "dotenv_compat" => {
                let map = dotenv_compat::parse(black_box(source.as_bytes()));
                let len = map.len();
                black_box(map);
                len
            }
            "dotenv_ng" => {
                let map = dotenv_ng_core::EnvLoader::with_reader(Cursor::new(black_box(
                    source.as_bytes(),
                )))
                .sequence(dotenv_ng_core::EnvSequence::InputOnly)
                .load()
                .unwrap();
                let len = map.len();
                black_box(map);
                len
            }
            "dotenv_parser" => {
                let map = dotenv_parser::parse_dotenv(black_box(&source)).unwrap();
                let len = map.len();
                black_box(map);
                len
            }
            "adotenv_values" => {
                let values = load_values([black_box(path.as_str())], Dialect::PORTABLE).unwrap();
                let len = values.len();
                black_box(values);
                len
            }
            "adotenv_load_default" => {
                let loaded = load_files([black_box(path.as_str())], Dialect::PORTABLE).unwrap();
                let len = loaded.len();
                black_box(loaded);
                len
            }
            "adotenv_load_full" => {
                let loaded = load_files_with_options(
                    [black_box(path.as_str())],
                    LoadOptions::full(Dialect::PORTABLE),
                )
                .unwrap();
                let len = loaded.len();
                black_box(loaded);
                len
            }
            other => panic!("unknown mode {other}"),
        };
    }

    let live = ALLOCATED.load(Ordering::Relaxed) - DEALLOCATED.load(Ordering::Relaxed);
    let peak = PEAK.load(Ordering::Relaxed);
    println!(
        "{} len={} live={} peak={} allocs={}",
        mode,
        total / iterations,
        live.saturating_sub(base_live),
        peak.saturating_sub(base_peak),
        ALLOCATED.load(Ordering::Relaxed) / iterations
    );
}
