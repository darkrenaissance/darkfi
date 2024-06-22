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

            let r = (data[idx] as f32) / 255.;
            let g = (data[idx + 1] as f32) / 255.;
            let b = (data[idx + 2] as f32) / 255.;
            let a = (data[idx + 3] as f32) / 255.;

            let r = (a * r * 255.) as u8;
            let g = (a * g * 255.) as u8;
            let b = (a * b * 255.) as u8;

            let val = "█".truecolor(r, g, b).to_string();
            out.push_str(&val);
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
