pub mod dir_listing;
pub mod logger;
pub mod models;
pub mod utils;
pub use dir_listing::{list_directory, search_and_display_interactive};

use clap::Parser;
use log::info;
use logger::init_logger;
use std::path::Path;

use models::Cli;

fn main() -> Result<(), anyhow::Error> {
    init_logger();
    let start_time = std::time::Instant::now();
    let args = Cli::parse();
    let path = Path::new(&args.dir);

    // 检查是否启用了交互式搜索模式
    if let Some(pattern) = &args.search {
        if path.is_dir() {
            search_and_display_interactive(path, pattern)?;
        } else {
            println!("错误: 路径不是目录: {}", path.display());
        }
    } else if path.is_dir() {
        list_directory(path, &args);
    } else {
        println!("{}", path.display());
    }

    // if args.show_time {
    //     let duration = start_time.elapsed();
    //     log::warn!(
    //         "\n运行时间: {:.6}秒 ({}毫秒, {}纳秒)",
    //         duration.as_secs_f64(),
    //         duration.as_millis(),
    //         duration.as_nanos()
    //     );
    // }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
