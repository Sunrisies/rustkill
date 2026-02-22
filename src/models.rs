#[derive(Debug, Clone)]
pub enum DeleteStatus {
    NotDeleted, // 未删除
    Deleting,   // 删除中
    Deleted,    // 删除结束
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub file_type: char,
    pub permissions: String,
    pub size_display: String,
    pub size_raw: u64,
    pub path: String,
    pub delete_status: DeleteStatus, // 使用枚举代替简单的布尔值
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
}

#[derive(Debug)]
pub struct DirEntry {
    pub path: PathBuf,
    pub size: u64,
    pub is_directory: bool,
}
