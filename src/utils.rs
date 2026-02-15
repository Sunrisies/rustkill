use indicatif::{ProgressBar, ProgressStyle};
pub fn human_readable_size(bytes: u64) -> String {
    // 定义单位数组
    let units = ["B", "KB", "MB", "GB", "TB"];
    // 将字节数转换为浮点数
    let mut size = bytes as f64;
    // 初始化单位索引
    let mut unit = 0;

    while size >= 1024.0 && unit < units.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    // 添加目录大小处理
    if bytes == 0 {
        return String::from("0B");
    }

    format!("{:.1}{}", size, units[unit])
}

// 引入 ProgressBar 类型，假设它来自 indicatif 库
pub fn progress_bar_init(
    total_files: Option<u64>,
) -> Result<ProgressBar, Box<dyn std::error::Error>> {
    let pb = match total_files {
        Some(total) => ProgressBar::new(total),
        None => ProgressBar::new_spinner(),
    };

    // 修改进度条样式模板
    let style = match total_files {
        Some(_) => ProgressStyle::default_bar().template("{spinner:.green} {msg}")?,
        None => ProgressStyle::default_spinner().template("{spinner:.green} {msg}")?,
    };

    pb.set_style(style.progress_chars("#>-"));
    Ok(pb)
}
