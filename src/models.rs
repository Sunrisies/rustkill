// use clap::Parser;

// #[derive(Parser, Debug)]
// #[command(
//     version,
//     author,
//     about = "一个类似 ls 的命令行目录列表工具",
//     long_about = "用法示例:\n  ll -l 查看详细列表\n  ll -a 显示隐藏文件"
// )]
// pub struct Cli {
//     /// 指定要列出的文件或目录
//     #[arg(default_value = ".", value_name = "FILE")]
//     pub file: String,

//     /// 启用详细模式
//     #[arg(short = 'l', long = "long", help = "使用长列表格式")]
//     pub long_format: bool,

//     /// 启用人类可读的文件大小
//     #[arg(
//         short = 'H',
//         long = "human-readable",
//         help = "使用易读的文件大小格式 (例如 1K, 234M, 2G)"
//     )] // 修改短选项为 -H
//     pub human_readable: bool,

//     /// 显示隐藏文件
//     #[arg(short = 'a', long = "all", help = "不要忽略以开头的条目 .")]
//     pub all: bool,

//     /// 显示程序运行时间
//     #[arg(short = 't', long = "time")]
//     pub show_time: bool,

//     /// 启用并行处理加速扫描
//     #[arg(short = 'f', long = "fast")]
//     pub parallel: bool,

//     /// 按文件大小排序
//     #[arg(short = 's', long = "sort")]
//     pub sort: bool,

//     /// 搜索文件名或目录名
//     #[arg(long = "name", value_name = "PATTERN")]
//     pub name: Option<String>,

//     /// 显示全部路径
//     #[arg(short = 'p', long = "full-path", help = "显示全部路径")]
//     pub full_path: bool,
// }
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub file_type: char,
    pub permissions: String,
    pub size_display: String,
    pub size_raw: u64,
    pub path: String,
}

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    version,
    author,
    about = "一个用于清理项目目录的工具",
    long_about = "用法示例:\n  clean -d node_modules 删除 node_modules 目录\n  clean -d target 删除 target 目录\n  clean -d node_modules -d target 同时删除多个目录\n  clean -s pattern 交互式搜索并显示结果"
)]
pub struct Cli {
    /// 指定要清理的根目录
    #[arg(default_value = ".", value_name = "DIR")]
    pub dir: String,

    /// 指定要删除的目录名称
    #[arg(short = 'd', long = "dir", value_name = "NAME",
    default_values_t = vec!["target".to_string(), "node_modules".to_string()],
    help = "指定要删除的目录名称（默认: target, node_modules）")]
    pub dirs_to_delete: Vec<String>,

    /// 显示将要删除的目录，但不实际删除（干运行模式）
    #[arg(
        short = 'n',
        long = "dry-run",
        help = "显示将要删除的目录，但不实际删除"
    )]
    pub dry_run: bool,

    /// 显示详细输出
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// 强制删除，不提示确认
    #[arg(short = 'f', long = "force", help = "强制删除，不提示确认")]
    pub force: bool,

    /// 递归删除子目录中的匹配项
    #[arg(short = 'r', long = "recursive", help = "递归删除子目录中的匹配项")]
    pub recursive: bool,

    /// 交互式搜索并显示结果
    #[arg(
        short = 's',
        long = "search",
        value_name = "PATTERN",
        help = "交互式搜索并显示结果"
    )]
    pub search: Option<String>,
}

#[derive(Debug)]
pub struct DirEntry {
    pub path: PathBuf,
    pub size: u64,
    pub is_directory: bool,
}
