use colored::Colorize;

pub fn ansi_texture(width: usize, height: usize, data: &Vec<u8>) -> String {
    let mut out = String::new();

    out.push('┌');
    for j in 0..width {
        out.push('─');
    }
    out.push('┐');
    out.push('\n');

    for i in 0..height {
        out.push('│');
        for j in 0..width {
            let idx = 4 * (i * width + j);

            let r = data[idx];
            let g = data[idx + 1];
            let b = data[idx + 2];
            let a = data[idx + 3];

            #[cfg(target_os = "android")]
            {
                if a > 204 {
                    out.push('█');
                } else if a > 153 {
                    out.push('▓');
                } else if a > 102 {
                    out.push('▒');
                } else if a > 51 {
                    out.push('░');
                } else {
                    out.push(' ');
                }
            }

            #[cfg(target_os = "linux")]
            {
                let r = ((a as f32 * r as f32) / 255.) as u8;
                let g = ((a as f32 * g as f32) / 255.) as u8;
                let b = ((a as f32 * b as f32) / 255.) as u8;

                let val = "█".truecolor(r, g, b).to_string();
                out.push_str(&val);
            }
        }
        out.push('│');
        out.push('\n');
    }

    out.push('└');
    for j in 0..width {
        out.push('─');
    }
    out.push('┘');
    out.push('\n');

    out
}
