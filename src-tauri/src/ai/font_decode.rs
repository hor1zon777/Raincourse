//! 运行时解混淆雨课堂动态加密字体。
//!
//! 雨课堂题干用 `xuetangx-com-encrypted-font` 字体把一批常用字的字形在码点间随机重排：
//! 题干文本里是「诊岁充诊色…」等乱码码点，浏览器用该字体渲染才显示成真字。每个字体文件
//! hash 都不同、同一码点映射到的真字 99.9% 互不相同——无法用固定映射表硬编码。
//!
//! 本模块运行时下载该字体，用内嵌的**同源思源黑体 ExtraLight**（雨课堂混淆字体即由它
//! 子集化而来，name 表为证）作参照，逐字栅格化 + 最近邻字形比对，动态还原
//! 「混淆码点 → 真字」。候选集就是混淆字体自身的码点集（这批字的字形被互相重排）。
//!
//! 安全原则：最近邻置信不足或参照里缺该字时不写入映射，上层据此「跳过该题」，
//! 绝不把可能解错的题干发给 AI。

use std::collections::HashMap;

use fontdue::{Font, FontSettings};

use crate::error::AppError;

/// 内嵌参照字体：思源黑体 SC ExtraLight（GB2312 子集，与雨课堂混淆字体同源同字重）。
static REFERENCE_FONT: &[u8] =
    include_bytes!("../../assets/source-han-sans-sc-extralight-gb2312.otf");

/// 字形归一化网格边长（渲染后缩放到 GRID×GRID 灰度再比对）。
const GRID: usize = 48;
/// 栅格化字号（px），足够区分字形细节又不过慢。
const RENDER_PX: f32 = 64.0;
/// 置信判定：最近邻距离需 ≤ 次近邻距离 × 该比值才采纳，否则视为未知，
/// 避免「天/失/夫」等近形字误判后把错题发给 AI。
const CONFIDENCE_RATIO: f32 = 0.95;

/// 运行时生成的「混淆码点 → 真字」映射。
pub struct FontDecodeMap {
    map: HashMap<char, char>,
}

impl FontDecodeMap {
    /// 解码单字：命中返回真字；未覆盖或低置信返回 `None`（上层按未知字符跳过该题）。
    pub fn decode_char(&self, c: char) -> Option<char> {
        self.map.get(&c).copied()
    }

    /// 已解出的字数（日志/诊断用）。
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// 测试辅助：从固定映射对构造。
    #[cfg(test)]
    pub fn from_pairs(pairs: &[(char, char)]) -> Self {
        Self {
            map: pairs.iter().copied().collect(),
        }
    }
}

/// 下载字体并构建解码映射。下载/解析失败返回 `Err`，
/// 由上层让该测验所有加密题走「跳过」护栏。
pub async fn build_decode_map(font_url: &str) -> Result<FontDecodeMap, AppError> {
    let bytes = download_font(font_url).await?;
    build_decode_map_from_bytes(&bytes)
}

async fn download_font(url: &str) -> Result<Vec<u8>, AppError> {
    // 字体托管在 fe-static CDN，无需雨课堂 cookie；用独立 client。
    let resp = reqwest::Client::new().get(url).send().await?;
    if !resp.status().is_success() {
        return Err(AppError::ApiError(format!(
            "下载加密字体失败: HTTP {}",
            resp.status()
        )));
    }
    Ok(resp.bytes().await?.to_vec())
}

/// 比对核心：渲染混淆字体每个码点，与参照字体同码点集的标准字形做最近邻匹配。
fn build_decode_map_from_bytes(obf_bytes: &[u8]) -> Result<FontDecodeMap, AppError> {
    let obf = Font::from_bytes(obf_bytes, FontSettings::default())
        .map_err(|e| AppError::ApiError(format!("解析加密字体失败: {e}")))?;
    let reference = Font::from_bytes(REFERENCE_FONT, FontSettings::default())
        .map_err(|e| AppError::ApiError(format!("解析参照字体失败: {e}")))?;

    // 候选集 = 混淆字体 cmap 的码点集（这批字的真字字形被互相重排）。
    let codepoints: Vec<char> = obf.chars().keys().copied().collect();

    // 预渲染：参照字形（候选集）+ 混淆字形（查询集），脱离 Font 仅保留位图，
    // 便于把 O(N²) 的最近邻比对放到多线程里跑（参照里缺的字自动落选 → 可能成为未知）。
    let candidates: Vec<(char, Vec<f32>)> = codepoints
        .iter()
        .filter_map(|&c| rasterize_normalized(&reference, c).map(|bmp| (c, bmp)))
        .collect();
    let queries: Vec<(char, Vec<f32>)> = codepoints
        .iter()
        .filter_map(|&c| rasterize_normalized(&obf, c).map(|bmp| (c, bmp)))
        .collect();

    let map: HashMap<char, char> = match_queries_parallel(&queries, &candidates)
        .into_iter()
        .collect();
    Ok(FontDecodeMap { map })
}

/// 单个查询字形在候选集里找最近邻：最近邻显著优于次近邻才采纳（否则视为未知 → 上层跳过该题）。
fn match_one(query: &(char, Vec<f32>), candidates: &[(char, Vec<f32>)]) -> Option<(char, char)> {
    let (c, q) = query;
    let (mut best, mut best_d, mut second_d) = (None, f32::INFINITY, f32::INFINITY);
    for (cand_c, cand_bmp) in candidates {
        let d = l2(q, cand_bmp);
        if d < best_d {
            second_d = best_d;
            best_d = d;
            best = Some(*cand_c);
        } else if d < second_d {
            second_d = d;
        }
    }
    best.and_then(|real| {
        if second_d.is_infinite() || best_d <= CONFIDENCE_RATIO * second_d {
            Some((*c, real))
        } else {
            None
        }
    })
}

/// 把查询集按可用核数分块，用 `std::thread::scope` 并行做最近邻匹配后合并。
/// 候选集 / 查询集均为已栅格化的只读位图，线程间共享引用即可（无新依赖）。
fn match_queries_parallel(
    queries: &[(char, Vec<f32>)],
    candidates: &[(char, Vec<f32>)],
) -> Vec<(char, char)> {
    let n = queries.len();
    if n == 0 {
        return Vec::new();
    }
    let workers = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
        .min(n);
    if workers <= 1 {
        return queries
            .iter()
            .filter_map(|q| match_one(q, candidates))
            .collect();
    }
    let chunk = (n + workers - 1) / workers;
    std::thread::scope(|scope| {
        let handles: Vec<_> = queries
            .chunks(chunk)
            .map(|qs| {
                scope.spawn(move || {
                    qs.iter()
                        .filter_map(|q| match_one(q, candidates))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut out = Vec::with_capacity(n);
        for h in handles {
            if let Ok(part) = h.join() {
                out.extend(part);
            }
        }
        out
    })
}

/// 渲染字符为 GRID×GRID 灰度向量：取字形紧致位图后缩放到统一网格，
/// 自动消除字号/位置差异（与生成内嵌字体时的离线验证流程一致）。
fn rasterize_normalized(font: &Font, c: char) -> Option<Vec<f32>> {
    let (m, bitmap) = font.rasterize(c, RENDER_PX);
    if m.width == 0 || m.height == 0 || bitmap.is_empty() {
        return None;
    }
    let (w, h) = (m.width, m.height);
    let mut out = vec![0f32; GRID * GRID];
    for gy in 0..GRID {
        let sy = (gy * h / GRID).min(h - 1);
        for gx in 0..GRID {
            let sx = (gx * w / GRID).min(w - 1);
            out[gy * GRID + gx] = bitmap[sy * w + sx] as f32;
        }
    }
    Some(out)
}

fn l2(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_font_parses_and_covers_common_chars() {
        let f = Font::from_bytes(REFERENCE_FONT, FontSettings::default())
            .expect("内嵌参照字体应能被 fontdue 解析");
        // GB2312 常用字应在 subset 内（lookup_glyph_index 返回 0 表示缺失）。
        for c in ['电', '影', '和', '视', '的'] {
            assert_ne!(f.lookup_glyph_index(c), 0, "subset 应覆盖常用字 {c}");
        }
    }

    #[test]
    fn rasterize_normalized_is_fixed_size_and_nonempty() {
        let f = Font::from_bytes(REFERENCE_FONT, FontSettings::default()).unwrap();
        let bmp = rasterize_normalized(&f, '电').expect("应渲染出字形");
        assert_eq!(bmp.len(), GRID * GRID);
        assert!(bmp.iter().any(|&v| v > 0.0), "字形不应全空白");
    }

    #[test]
    fn same_glyph_distance_is_smaller_than_different() {
        let f = Font::from_bytes(REFERENCE_FONT, FontSettings::default()).unwrap();
        let dian = rasterize_normalized(&f, '电').unwrap();
        let dian2 = rasterize_normalized(&f, '电').unwrap();
        let ying = rasterize_normalized(&f, '影').unwrap();
        assert!(
            l2(&dian, &dian2) < l2(&dian, &ying),
            "同字距离应小于异字距离"
        );
    }

    #[test]
    fn decode_char_lookup() {
        let m = FontDecodeMap::from_pairs(&[('\u{4E00}', '电'), ('\u{4E07}', '影')]);
        assert_eq!(m.decode_char('\u{4E00}'), Some('电'));
        assert_eq!(m.decode_char('\u{4E07}'), Some('影'));
        assert_eq!(m.decode_char('X'), None);
        assert_eq!(m.len(), 2);
    }

    /// 本地验证：用真实混淆字体样本对照 font_map oracle，确认运行时解码准确率
    /// （也验证 fontdue 对可变字体 VF 的渲染）。依赖 tmp/ 样本，故 ignore；
    /// 用 `cargo test --lib -- --ignored --nocapture` 手动跑。
    #[test]
    #[ignore = "依赖 tmp/ 真实样本，手动跑验证准确率"]
    fn accuracy_against_real_obfuscated_font() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../tmp/");
        let obf = match std::fs::read(format!("{dir}exam_font.ttf")) {
            Ok(b) => b,
            Err(_) => {
                println!("跳过：未找到 tmp/exam_font.ttf");
                return;
            }
        };
        let oracle: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(format!("{dir}font_map.json")).unwrap())
                .unwrap();
        let map = build_decode_map_from_bytes(&obf).expect("应能构建解码表");

        let entries = oracle.as_array().unwrap();
        let (mut adopted, mut agree) = (0usize, 0usize);
        for e in entries {
            let from = char::from_u32(e["from"].as_u64().unwrap() as u32).unwrap();
            let to = char::from_u32(e["to"].as_u64().unwrap() as u32).unwrap();
            if let Some(got) = map.decode_char(from) {
                adopted += 1;
                if got == to {
                    agree += 1;
                }
            }
        }
        let acc = agree as f64 / adopted.max(1) as f64 * 100.0;
        let cov = adopted as f64 / entries.len() as f64 * 100.0;
        println!(
            "Rust 解码 vs font_map oracle：采纳 {adopted}/{}（覆盖 {cov:.1}%），一致 {agree}（准确 {acc:.1}%）",
            entries.len()
        );
        assert!(acc >= 90.0, "准确率 {acc:.1}% 低于阈值");
    }
}
