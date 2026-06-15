//! 章节测验题目解析、题库匹配与 AI 答案编码。
//!
//! 字段路径已用真实样本 `quiz_74943199.json`（MOOC exercise 平台）校准：
//! - 题目数组：`data.problems[]`
//! - 题目内容嵌在 `content` 子对象：`content.Body` / `content.Options[{key,value}]`
//!   / `content.ProblemType`(数字) / `content.TypeText`(中文) / `content.Type`(英文)
//! - 题型数字：**1=单选、2=多选、6=判断**（填空数字暂无样本，靠中文/英文兜底）
//! - 题目 `problem_id` 在题目顶层（数字），`content.ProblemID` 同值兜底
//! - **正确答案在 `problems[i].user.answer`（字符串数组）**：
//!   单选 `["A"]`、多选 `["A","B","C","D"]`、判断 `["true"]`/`["false"]`
//!
//! 提交编码以 `user.answer` 的**同平台第一手形状**为准 → 一律产出**字符串数组**，
//! 题库模式直接规范化透传、AI 模式编码成同形状，两路最终格式统一。
//!
//! 加密字体解混淆：题干 `xuetangx-com-encrypted-font` span 内是被打乱的码点，
//! 由调用方传入运行时构建的 [`FontDecodeMap`]（见 [`crate::ai::font_decode`]）按字还原；
//! span 外的康熙部首兼容字（如 `⼏⽅⾯`）是题目原文，单独做 Unicode 规范化。
//!
//! 安全原则：任何字段提取 / 编码失败都返回 `None`/`Err`，由上层「跳过该题、不提交」，
//! 绝不用猜测出的脏数据发起提交。

use serde_json::{json, Value};

use crate::ai::font_decode::FontDecodeMap;

/// 客观题题型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemType {
    SingleChoice,
    MultiChoice,
    Judge,
    FillBlank,
}

// ---- 字段名 / 路径候选（待联调可在此增改）----
/// 题目 `problem_id` 候选（题目顶层优先，content 内兜底）。
const PROBLEM_ID_KEYS: &[&str] = &["problem_id", "ProblemID", "id"];
/// 题型字段候选（位于 content 节点；中文 TypeText / 英文 Type / 数字 ProblemType）。
const TYPE_KEYS: &[&str] = &[
    "TypeText",
    "Type",
    "ProblemType",
    "type_text",
    "problem_type",
    "type",
];
/// 题干字段候选（位于 content 节点）。
const BODY_KEYS: &[&str] = &["Body", "body", "content", "stem", "title"];
/// 选项列表字段候选（位于 content 节点）。
const OPTION_LIST_KEYS: &[&str] = &["Options", "options"];
/// 选项文本字段候选。
const OPTION_TEXT_KEYS: &[&str] = &["value", "content", "text", "option", "body", "title"];
/// 选项标签字段候选（真实样本为 `key`：A/B/C/D 或 true/false）。
const OPTION_LABEL_KEYS: &[&str] = &["key", "label", "Key", "index"];
/// 题库里每题「正确答案」字段候选（题目顶层 / content 内的兜底；主路径见 `extract_correct_answer`）。
const ANSWER_KEYS: &[&str] = &[
    "answer",
    "Answer",
    "correct_answer",
    "std_answer",
    "answers",
];
/// 题目数组路径候选（相对 get_exercise_list 返回的根）。
const PROBLEMS_PATHS: &[&[&str]] = &[
    &["data", "problems"],
    &["data", "exercises"],
    &["data", "problem_list"],
    &["problems"],
];

/// 题目内容节点：优先 `content` 子对象（真实结构），否则退回题目对象自身（兼容扁平结构）。
fn content_node(q: &Value) -> &Value {
    match q.get("content") {
        Some(c) if c.is_object() => c,
        _ => q,
    }
}

/// 取字段为字符串：字符串原样、数字转字符串、其它返回 None。
fn pick_str(v: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        match v.get(*k) {
            Some(Value::String(s)) if !s.is_empty() => return Some(s.clone()),
            Some(n) if n.is_number() => return Some(n.to_string()),
            _ => {}
        }
    }
    None
}

impl ProblemType {
    /// 从题目对象推断题型。先看 content 节点的中文 `TypeText` / 英文 `Type`，
    /// 再看数字 `ProblemType`（1=单选、2=多选、6=判断；4 视为填空兜底）。
    pub fn from_question(q: &Value) -> Option<ProblemType> {
        let c = content_node(q);

        // 1) 字符串：中文 TypeText / 英文 Type
        if let Some(s) = pick_str(c, TYPE_KEYS) {
            let s = s.trim();
            if s.contains("单选") || s.eq_ignore_ascii_case("SingleChoice") {
                return Some(ProblemType::SingleChoice);
            }
            if s.contains("多选") || s.eq_ignore_ascii_case("MultipleChoice") {
                return Some(ProblemType::MultiChoice);
            }
            if s.contains("判断")
                || s.eq_ignore_ascii_case("Judgement")
                || s.eq_ignore_ascii_case("Judgment")
            {
                return Some(ProblemType::Judge);
            }
            if s.contains("填空")
                || s.eq_ignore_ascii_case("BlankFilling")
                || s.eq_ignore_ascii_case("FillBlank")
            {
                return Some(ProblemType::FillBlank);
            }
        }

        // 2) 数字 ProblemType（真实取值：1/2/6）
        for k in ["ProblemType", "problem_type", "type"] {
            if let Some(n) = c.get(k).and_then(|x| x.as_i64()) {
                return match n {
                    1 => Some(ProblemType::SingleChoice),
                    2 => Some(ProblemType::MultiChoice),
                    6 => Some(ProblemType::Judge),
                    // 4 暂作填空兜底（无真实样本，待联调）
                    4 => Some(ProblemType::FillBlank),
                    _ => None,
                };
            }
        }
        None
    }
}

/// 提取题目 `problem_id`（字符串形式）：题目顶层优先，content.ProblemID 兜底。
pub fn extract_problem_id(q: &Value) -> Option<String> {
    pick_str(q, PROBLEM_ID_KEYS).or_else(|| pick_str(content_node(q), PROBLEM_ID_KEYS))
}

/// 判断该小题「当前用户是否已提交过」（用于自动答题跳过已答题）。
///
/// 真实样本（`quiz_74943199.json`）证实：`user.answer` 是**标准答案**（常因
/// `is_show_answer` 提前公布），**不能**用来判断是否作答；可靠信号是
/// `user.my_count`（提交次数）与 `user.submit_time`（提交时间）。
/// `user.my_answers` 在单选/判断题即便已答也可能为 `null`，故仅作兜底。
pub fn is_answered(q: &Value) -> bool {
    let user = match q.get("user") {
        Some(u) if u.is_object() => u,
        _ => return false,
    };
    // 1) 提交次数 ≥ 1（兼容数字或数字字符串）
    let my_count = user.get("my_count").and_then(|v| match v {
        Value::String(s) => s.trim().parse::<i64>().ok(),
        n if n.is_number() => n.as_i64(),
        _ => None,
    });
    if matches!(my_count, Some(c) if c >= 1) {
        return true;
    }
    // 2) 提交时间非空
    if let Some(Value::String(s)) = user.get("submit_time") {
        if !s.trim().is_empty() {
            return true;
        }
    }
    // 3) 兜底：my_answers 非空
    match user.get("my_answers") {
        Some(Value::Object(o)) => !o.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        _ => false,
    }
}

/// 提取题干纯文本（去 HTML 标签；无字体解码场景）。
pub fn extract_body(q: &Value) -> String {
    extract_body_with_decoder(q, None)
}

/// 按运行时字体解码器提取题干纯文本。
pub fn extract_body_with_decoder(q: &Value, decoder: Option<&FontDecodeMap>) -> String {
    let raw = pick_str(content_node(q), BODY_KEYS).unwrap_or_default();
    strip_html_with_decoder(&raw, decoder)
}

/// 提取选项列表 `[(label, text)]`。label 取选项的 `key`（真实即 A/B/C/D，
/// 判断题为 true/false）；缺失时按序补 A/B/C…。text 取选项文本字段并去 HTML。
pub fn extract_options(q: &Value) -> Vec<(String, String)> {
    extract_options_with_decoder(q, None)
}

/// 按运行时字体解码器提取选项。
pub fn extract_options_with_decoder(
    q: &Value,
    decoder: Option<&FontDecodeMap>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let c = content_node(q);
    let list = OPTION_LIST_KEYS
        .iter()
        .find_map(|k| c.get(*k).and_then(|v| v.as_array()));
    if let Some(arr) = list {
        for (i, opt) in arr.iter().enumerate() {
            if let Some(s) = opt.as_str() {
                // 纯字符串选项：按序生成字母标签
                let label = ((b'A' + (i.min(25) as u8)) as char).to_string();
                out.push((label, strip_html_with_decoder(s, decoder)));
                continue;
            }
            let label = pick_str(opt, OPTION_LABEL_KEYS)
                .unwrap_or_else(|| ((b'A' + (i.min(25) as u8)) as char).to_string());
            let text = pick_str(opt, OPTION_TEXT_KEYS).unwrap_or_default();
            out.push((label, strip_html_with_decoder(&text, decoder)));
        }
    }
    out
}

/// 判断题目 HTML 中是否使用了雨课堂加密字体。
pub fn question_uses_encrypted_font(q: &Value) -> bool {
    raw_html_fields(q).iter().any(|s| {
        s.to_ascii_lowercase()
            .contains("xuetangx-com-encrypted-font")
    })
}

/// 返回加密字体 span 中当前解码器无法覆盖的字符（去重，最多由调用方截断展示）。
pub fn encrypted_font_unknown_chars(q: &Value) -> Vec<char> {
    encrypted_font_unknown_chars_with_decoder(q, None)
}

/// 按运行时解码器检测加密字体 span 内未能还原的字符。
pub fn encrypted_font_unknown_chars_with_decoder(
    q: &Value,
    decoder: Option<&FontDecodeMap>,
) -> Vec<char> {
    let mut out = Vec::new();
    for raw in raw_html_fields(q) {
        for c in collect_unknown_encrypted_chars(&raw, decoder) {
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out
}

fn raw_html_fields(q: &Value) -> Vec<String> {
    let mut fields = Vec::new();
    let c = content_node(q);
    if let Some(raw) = pick_str(c, BODY_KEYS) {
        fields.push(raw);
    }
    if let Some(arr) = OPTION_LIST_KEYS
        .iter()
        .find_map(|k| c.get(*k).and_then(|v| v.as_array()))
    {
        for opt in arr {
            if let Some(s) = opt.as_str() {
                fields.push(s.to_string());
            } else if let Some(text) = pick_str(opt, OPTION_TEXT_KEYS) {
                fields.push(text);
            }
        }
    }
    fields
}

/// 找到题目数组（容错多路径）。`root` 为 get_exercise_list 返回的根。
pub fn find_problems(root: &Value) -> Option<Vec<&Value>> {
    for path in PROBLEMS_PATHS {
        let mut cur = root;
        let mut ok = true;
        for seg in *path {
            match cur.get(*seg) {
                Some(next) => cur = next,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            if let Some(arr) = cur.as_array() {
                return Some(arr.iter().collect());
            }
        }
    }
    None
}

/// 汇总一份导出 quiz_json 的得分。
///
/// 返回 `(得分, 满分, 已答数, 题数)`：满分 = Σ `content.Score`（数字）；
/// 得分 = Σ `user.my_score`（字符串如 "1.00"，可解析才计入并计一次已答）；
/// 题数 = problems 长度。`exported` 为 `{answer, info}` 容器或直接的 exercise 返回。
pub fn summarize_quiz(exported: &Value) -> (f64, f64, usize, usize) {
    let root = exported.get("answer").unwrap_or(exported);
    let problems = match find_problems(root) {
        Some(p) => p,
        None => return (0.0, 0.0, 0, 0),
    };
    let count = problems.len();
    let mut score = 0.0;
    let mut total = 0.0;
    let mut answered = 0usize;
    for p in problems {
        if let Some(s) = content_node(p).get("Score").and_then(|v| v.as_f64()) {
            total += s;
        }
        let my = p
            .get("user")
            .and_then(|u| u.get("my_score"))
            .and_then(|v| match v {
                Value::String(s) => s.trim().parse::<f64>().ok(),
                n if n.is_number() => n.as_f64(),
                _ => None,
            });
        if let Some(v) = my {
            score += v;
            answered += 1;
        }
    }
    (score, total, answered, count)
}

/// 题库匹配：在导出题库里按 `problem_id` 找到对应题，取其「正确答案」并规范成提交格式。
///
/// `exported` 为 `quiz_{leaf_id}.json` 解析出的 `{ answer, info }` 容器；其内
/// `answer` 才是 get_exercise_list 的原始返回（无包装时直接用 `exported`）。
/// 命中题目但答案为空（如测验未公布答案）返回 `None`，由上层走 AI 兜底。
pub fn lookup_local(problem_id: &str, exported: &Value) -> Option<Value> {
    let answer_root = exported.get("answer").unwrap_or(exported);
    let problems = find_problems(answer_root)?;
    for p in problems {
        if extract_problem_id(p).as_deref() == Some(problem_id) {
            return extract_correct_answer(p);
        }
    }
    None
}

/// 取该题「正确答案」原始值并规范成提交数组：主路径 `user.answer`，
/// 兜底题目顶层 / content 内的 answer 候选字段。
fn extract_correct_answer(p: &Value) -> Option<Value> {
    let raw = p
        .get("user")
        .and_then(|u| ANSWER_KEYS.iter().find_map(|k| u.get(*k)))
        .or_else(|| ANSWER_KEYS.iter().find_map(|k| p.get(*k)))
        .or_else(|| {
            let c = content_node(p);
            ANSWER_KEYS.iter().find_map(|k| c.get(*k))
        })?;
    normalize_answer(raw)
}

/// 把读取到的答案规范成提交用字符串数组（去重保序、过滤空串）：
/// - 数组：逐项转字符串
/// - 字符串：按 `; , 、` 及空白拆分（单选/判断通常单元素）
/// - 对象（疑似填空 `{"1":["x"]}`）：原样返回
/// 规范后为空则返回 `None`。
fn normalize_answer(raw: &Value) -> Option<Value> {
    match raw {
        Value::Object(_) => Some(raw.clone()), // 填空 dict 等结构原样透传
        Value::Array(arr) => {
            let items: Vec<String> = arr
                .iter()
                .filter_map(|x| match x {
                    Value::String(s) => Some(s.trim().to_string()),
                    n if n.is_number() => Some(n.to_string()),
                    _ => None,
                })
                .filter(|s| !s.is_empty())
                .collect();
            finalize_items(items)
        }
        Value::String(s) => {
            let items: Vec<String> = s
                .split([';', ',', '、', ' ', '\t', '\n'])
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect();
            finalize_items(items)
        }
        _ => None,
    }
}

/// 去重保序后包装成 JSON 字符串数组；为空返回 None。
fn finalize_items(items: Vec<String>) -> Option<Value> {
    let mut out: Vec<String> = Vec::new();
    for it in items {
        if !out.contains(&it) {
            out.push(it);
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(Value::Array(out.into_iter().map(Value::String).collect()))
    }
}

/// AI 输出文本 → 提交格式编码。解析失败返回 `Err`（上层跳过该题，不提交）。
///
/// 输出对齐 `user.answer` 的同平台形状（字符串数组）：
/// - 单选：`["A"]`
/// - 多选：`["A","C","D"]`（字母升序去重）
/// - 判断：`["true"]` / `["false"]`
/// - 填空：`{"1":["x"],...}`（dict；无真实样本，待联调）
#[allow(dead_code)]
pub fn encode_ai_answer(raw: &str, qtype: ProblemType) -> Result<Value, String> {
    encode_ai_answer_inner(raw, qtype, None)
}

/// 带选项标签校验的 AI 输出编码。
///
/// 选择题会只接受 `options` 中真实存在的单字母标签，避免把模型常见输出
/// `Answer: B` / `The correct answer is C` 中 `Answer`、`The` 等英文单词的首字母
/// 误解析成选项。若解析出多个候选单选项，则保守报错并由上层跳过，不猜测提交。
pub fn encode_ai_answer_with_options(
    raw: &str,
    qtype: ProblemType,
    options: &[(String, String)],
) -> Result<Value, String> {
    encode_ai_answer_inner(raw, qtype, Some(options))
}

fn encode_ai_answer_inner(
    raw: &str,
    qtype: ProblemType,
    options: Option<&[(String, String)]>,
) -> Result<Value, String> {
    let raw = raw.trim();
    match qtype {
        ProblemType::SingleChoice => {
            let allowed = choice_label_set(options);
            let letters = choice_letters(raw, allowed.as_ref());
            match letters.as_slice() {
                [letter] => Ok(json!([letter.to_string()])),
                [] => Err(format!("无法从 AI 输出解析单选答案: {:?}", raw)),
                _ => Err(format!(
                    "AI 单选输出包含多个候选选项 {:?}: {:?}",
                    letters, raw
                )),
            }
        }
        ProblemType::MultiChoice => {
            let allowed = choice_label_set(options);
            let letters = sorted_choice_letters(raw, allowed.as_ref());
            if letters.is_empty() {
                return Err(format!("无法从 AI 输出解析多选答案: {:?}", raw));
            }
            Ok(json!(letters
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()))
        }
        ProblemType::Judge => {
            let v =
                parse_judge(raw).ok_or_else(|| format!("无法从 AI 输出解析判断答案: {:?}", raw))?;
            Ok(json!([v]))
        }
        ProblemType::FillBlank => {
            parse_fill_blanks(raw).ok_or_else(|| format!("无法从 AI 输出解析填空答案: {:?}", raw))
        }
    }
}

/// 从真实选项里抽取可提交的单字母标签集合（通常为 A/B/C/D）。
fn choice_label_set(
    options: Option<&[(String, String)]>,
) -> Option<std::collections::BTreeSet<char>> {
    let mut set = std::collections::BTreeSet::new();
    if let Some(options) = options {
        for (label, _) in options {
            let trimmed = label.trim();
            let mut chars = trimmed.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                if c.is_ascii_alphabetic() {
                    set.insert(c.to_ascii_uppercase());
                }
            }
        }
    }
    if set.is_empty() {
        None
    } else {
        Some(set)
    }
}

fn sorted_choice_letters(s: &str, allowed: Option<&std::collections::BTreeSet<char>>) -> Vec<char> {
    let mut set = std::collections::BTreeSet::new();
    for c in choice_letters(s, allowed) {
        set.insert(c);
    }
    set.into_iter().collect()
}

/// 提取 AI 输出里的选项字母候选：
/// 1. 优先提取独立 token（`Answer: B` 只会得到 B，不会得到 Answer 的 A）；
/// 2. 若没有独立 token，则支持紧凑多选写法（`ACD`）。
fn choice_letters(s: &str, allowed: Option<&std::collections::BTreeSet<char>>) -> Vec<char> {
    let mut out = bounded_choice_letters(s, allowed);
    if out.is_empty() {
        out = compact_choice_letters(s, allowed);
    }
    out
}

fn bounded_choice_letters(
    s: &str,
    allowed: Option<&std::collections::BTreeSet<char>>,
) -> Vec<char> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    for (i, c) in chars.iter().enumerate() {
        if !c.is_ascii_alphabetic() {
            continue;
        }
        let up = c.to_ascii_uppercase();
        if !choice_allowed(up, allowed) {
            continue;
        }
        let prev_is_word = i > 0 && is_ascii_word(chars[i - 1]);
        let next_is_word = i + 1 < chars.len() && is_ascii_word(chars[i + 1]);
        if !prev_is_word && !next_is_word && !out.contains(&up) {
            out.push(up);
        }
    }
    out
}

fn compact_choice_letters(
    s: &str,
    allowed: Option<&std::collections::BTreeSet<char>>,
) -> Vec<char> {
    let letters: Vec<char> = s.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    if letters.is_empty() {
        return Vec::new();
    }
    // 紧凑写法只接受全大写，避免把 `AnswerB` 这类英文前缀压缩后误当成答案。
    if !letters.iter().all(|c| c.is_ascii_uppercase()) {
        return Vec::new();
    }

    let mut out = Vec::new();
    for c in letters {
        let up = c.to_ascii_uppercase();
        if !choice_allowed(up, allowed) {
            return Vec::new();
        }
        if !out.contains(&up) {
            out.push(up);
        }
    }
    out
}

fn choice_allowed(c: char, allowed: Option<&std::collections::BTreeSet<char>>) -> bool {
    allowed.map_or(c.is_ascii_alphabetic(), |set| set.contains(&c))
}

fn is_ascii_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// 解析判断题语义 → "true" / "false"（对齐 user.answer 形状）。
///
/// 先判否定（错误/否/×/false/B），再判肯定（正确/对/√/true/A），都不匹配返回 None。
fn parse_judge(s: &str) -> Option<&'static str> {
    let s = s.trim();
    let neg = s.contains('错')
        || s.contains('否')
        || s.contains('×')
        || s.contains('✗')
        || s.eq_ignore_ascii_case("false")
        || s.eq_ignore_ascii_case("f")
        || s.eq_ignore_ascii_case("b")
        || s.eq_ignore_ascii_case("no");
    if neg {
        return Some("false");
    }
    let pos = s.contains("正确")
        || s.contains('对')
        || s.contains('是')
        || s.contains('√')
        || s.contains('✓')
        || s.eq_ignore_ascii_case("true")
        || s.eq_ignore_ascii_case("t")
        || s.eq_ignore_ascii_case("a")
        || s.eq_ignore_ascii_case("yes");
    if pos {
        return Some("true");
    }
    None
}

/// 单空答案最大字符数：超过则视为解释 / 拒答而非填空答案（填空答案通常是词或短语）。
const MAX_BLANK_ANSWER_CHARS: usize = 100;

/// 解析填空：逐行 `序号=答案` → `{"1":["答案"]}`；无序号时整体作为第 1 空。
///
/// 安全校验（防止把 AI「看似成功、实为软失败」的输出当答案提交）：每个空都经
/// [`is_valid_blank_answer`] 校验——拒答 / 无关措辞（[`looks_like_refusal`]）或单空过长
/// （疑似解释）一律剔除；过滤后无任何有效空则返回 `None`，由上层「跳过该题、不提交」。
fn parse_fill_blanks(s: &str) -> Option<Value> {
    if s.trim().is_empty() {
        return None;
    }
    let mut map = serde_json::Map::new();
    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim();
            if !k.is_empty() && is_valid_blank_answer(v) {
                map.insert(k.to_string(), json!([v]));
            }
        }
    }
    if map.is_empty() {
        // 无 `序号=答案` 结构时整体作为第 1 空，仍需通过有效性校验。
        let v = s.trim();
        if !is_valid_blank_answer(v) {
            return None;
        }
        map.insert("1".to_string(), json!([v]));
    }
    Some(Value::Object(map))
}

/// 校验单个填空答案是否有效：非空、非拒答措辞、长度合理。
fn is_valid_blank_answer(v: &str) -> bool {
    let v = v.trim();
    !v.is_empty() && !looks_like_refusal(v) && v.chars().count() <= MAX_BLANK_ANSWER_CHARS
}

/// 识别 AI 拒答 / 无法作答 / 信息不足等无效输出（中英文常见措辞）。
///
/// 填空题不像选择 / 判断有标签或语义可校验，任何非空文本都会被当成答案，
/// 故在此拦截这类「HTTP 成功但内容无效」的软失败输出。
fn looks_like_refusal(s: &str) -> bool {
    let lower = s.to_lowercase();
    const REFUSAL_MARKERS: &[&str] = &[
        // 中文
        "无法回答",
        "无法作答",
        "无法确定",
        "无法提供",
        "无法解析",
        "无法判断",
        "无法得出",
        "无法识别",
        "不知道",
        "不清楚",
        "没有足够",
        "信息不足",
        "抱歉",
        "对不起",
        "无可奉告",
        // 英文（已小写，匹配 lower）
        "as an ai",
        "i cannot",
        "i can't",
        "i can not",
        "i'm sorry",
        "i am sorry",
        "unable to",
        "i'm not able",
        "i am not able",
        "cannot answer",
        "can't answer",
        "i don't know",
        "i do not know",
        "not enough information",
        "insufficient information",
    ];
    REFUSAL_MARKERS.iter().any(|m| lower.contains(m))
}

/// 少量 CJK Radical/Kangxi Radical 兼容字符规范化（题干里常见“⽅/⾯/⼏”）。
/// 这些不是加密字体混淆，而是题目原文使用的 Unicode 兼容字符，需全局规范化。
const COMPAT_RADICAL_MAP_PAIRS: &str = "\u{2F0F}\u{51E0}\u{2F45}\u{65B9}\u{2FAF}\u{9762}";

fn map_char_from_pairs(c: char, pairs: &str) -> Option<char> {
    let mut it = pairs.chars();
    while let (Some(from), Some(to)) = (it.next(), it.next()) {
        if from == c {
            return Some(to);
        }
    }
    None
}

fn normalize_compat_char(c: char) -> char {
    map_char_from_pairs(c, COMPAT_RADICAL_MAP_PAIRS).unwrap_or(c)
}

/// 是否落在可能被加密字体混淆的 CJK 区间（用于未知字符检测）。
fn is_potential_encrypted_char(c: char) -> bool {
    matches!(
        c as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0x2E80..=0x2FFF
    )
}

/// 收集加密字体 span 内、运行时解码器未能还原（且非康熙部首）的字符。
/// 解码器为 `None`（无字体 / 构建失败）时，span 内所有疑似混淆字符都算未知。
fn collect_unknown_encrypted_chars(s: &str, decoder: Option<&FontDecodeMap>) -> Vec<char> {
    let mut out = Vec::new();
    let mut encrypted_stack: Vec<bool> = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '<' {
            let mut tag = String::new();
            for tc in chars.by_ref() {
                if tc == '>' {
                    break;
                }
                tag.push(tc);
            }
            let lower = tag.trim().to_ascii_lowercase();
            if lower.starts_with('/') {
                let _ = encrypted_stack.pop();
                continue;
            }
            let parent_encrypted = encrypted_stack.last().copied().unwrap_or(false);
            let is_encrypted = parent_encrypted || lower.contains("xuetangx-com-encrypted-font");
            let self_closing = lower.ends_with('/')
                || lower.starts_with("br")
                || lower.starts_with("img")
                || lower.starts_with("input")
                || lower.starts_with('!');
            if !self_closing {
                encrypted_stack.push(is_encrypted);
            }
            continue;
        }

        if encrypted_stack.last().copied().unwrap_or(false)
            && is_potential_encrypted_char(c)
            && decoder.and_then(|d| d.decode_char(c)).is_none()
            && map_char_from_pairs(c, COMPAT_RADICAL_MAP_PAIRS).is_none()
            && !out.contains(&c)
        {
            out.push(c);
        }
    }
    out
}

fn decode_html_entities(s: String) -> String {
    s.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// HTML 清理：去标签 + 常见实体 + 仅对加密字体 span 内文本做运行时解码（无解码器场景）。
fn strip_html(s: &str) -> String {
    strip_html_with_decoder(s, None)
}

/// HTML 清理 + 加密字体解码。加密 span 内用 `decoder` 还原（解不出保留原字，交未知字符检测）；
/// span 外做康熙部首兼容字规范化。
fn strip_html_with_decoder(s: &str, decoder: Option<&FontDecodeMap>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut encrypted_stack: Vec<bool> = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '<' {
            let mut tag = String::new();
            for tc in chars.by_ref() {
                if tc == '>' {
                    break;
                }
                tag.push(tc);
            }
            let lower = tag.trim().to_ascii_lowercase();
            if lower.starts_with('/') {
                let _ = encrypted_stack.pop();
                if lower.starts_with("/p") || lower.starts_with("/div") || lower.starts_with("/br")
                {
                    out.push(' ');
                }
                continue;
            }
            if lower.starts_with("br") {
                out.push(' ');
            }
            if lower.starts_with('!') {
                continue;
            }
            let parent_encrypted = encrypted_stack.last().copied().unwrap_or(false);
            let is_encrypted = parent_encrypted || lower.contains("xuetangx-com-encrypted-font");
            let self_closing = lower.ends_with('/')
                || lower.starts_with("br")
                || lower.starts_with("img")
                || lower.starts_with("input");
            if !self_closing {
                encrypted_stack.push(is_encrypted);
            }
            continue;
        }

        if encrypted_stack.last().copied().unwrap_or(false) {
            // 加密 span 内：运行时解码器还原；解不出则保留原字（由未知字符检测捕获 → 跳过该题）。
            let decoded = decoder.and_then(|d| d.decode_char(c)).unwrap_or(c);
            out.push(decoded);
        } else {
            out.push(normalize_compat_char(c));
        }
    }

    decode_html_entities(out).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造贴近真实样本的题目对象。
    fn problem(problem_id: i64, ptype: i64, type_text: &str, user_answer: Value) -> Value {
        json!({
            "problem_id": problem_id,
            "content": {
                "Body": "<div><p>题干</p></div>",
                "Options": [
                    {"key":"A","value":"<p>甲</p>"},
                    {"key":"B","value":"<p>乙</p>"}
                ],
                "ProblemID": problem_id,
                "ProblemType": ptype,
                "TypeText": type_text
            },
            "user": { "answer": user_answer }
        })
    }

    #[test]
    fn problem_type_from_content_number_and_text() {
        // 数字：1/2/6（真实取值）
        assert_eq!(
            ProblemType::from_question(&json!({"content":{"ProblemType":1}})),
            Some(ProblemType::SingleChoice)
        );
        assert_eq!(
            ProblemType::from_question(&json!({"content":{"ProblemType":2}})),
            Some(ProblemType::MultiChoice)
        );
        assert_eq!(
            ProblemType::from_question(&json!({"content":{"ProblemType":6}})),
            Some(ProblemType::Judge)
        );
        // 中文 TypeText 兜底（填空无数字样本）
        assert_eq!(
            ProblemType::from_question(&json!({"content":{"TypeText":"填空题"}})),
            Some(ProblemType::FillBlank)
        );
        // 英文 Type 兜底
        assert_eq!(
            ProblemType::from_question(&json!({"content":{"Type":"SingleChoice"}})),
            Some(ProblemType::SingleChoice)
        );
        assert_eq!(
            ProblemType::from_question(&json!({"content":{"x":1}})),
            None
        );
    }

    #[test]
    fn extract_problem_id_top_level_and_content() {
        assert_eq!(
            extract_problem_id(&json!({"problem_id":65435327})).as_deref(),
            Some("65435327")
        );
        assert_eq!(
            extract_problem_id(&json!({"content":{"ProblemID":123}})).as_deref(),
            Some("123")
        );
    }

    #[test]
    fn is_answered_detects_prior_submission() {
        // 已答：my_count=1 + submit_time（真实样本形状，单选/判断 my_answers 为 null）
        assert!(is_answered(&json!({
            "user": {"answer":["A"], "my_count": 1, "my_answers": null, "submit_time": "2026-01-23 18:46"}
        })));
        // 已答：仅 submit_time 非空
        assert!(is_answered(&json!({
            "user": {"my_count": 0, "submit_time": "2026-01-23 18:46"}
        })));
        // 已答：仅 my_answers 非空（兜底）
        assert!(is_answered(&json!({
            "user": {"my_count": 0, "submit_time": "", "my_answers": {"B": true}}
        })));
        // 未答：my_count=0 / submit_time 空 / my_answers 空；user.answer 是标准答案不算已答
        assert!(!is_answered(&json!({
            "user": {"answer":["A","B"], "my_count": 0, "submit_time": "", "my_answers": null}
        })));
        // 未答：缺 user
        assert!(!is_answered(&json!({"problem_id": 1})));
        // 兼容数字字符串 my_count
        assert!(is_answered(&json!({"user": {"my_count": "2"}})));
    }

    #[test]
    fn extract_body_from_content() {
        let q = json!({"content":{"Body":"<div><p>这是&nbsp;题干</p></div>"}});
        assert_eq!(extract_body(&q), "这是 题干");
    }

    #[test]
    fn extract_options_uses_key_label() {
        let q = json!({"content":{"Options":[
            {"key":"A","value":"<p>甲</p>"},
            {"key":"B","value":"乙"}
        ]}});
        assert_eq!(
            extract_options(&q),
            vec![
                ("A".to_string(), "甲".to_string()),
                ("B".to_string(), "乙".to_string())
            ]
        );
    }

    #[test]
    fn extract_options_judge_true_false_keys() {
        let q = json!({"content":{"Options":[
            {"key":"true","value":""},
            {"key":"false","value":""}
        ]}});
        assert_eq!(
            extract_options(&q),
            vec![
                ("true".to_string(), "".to_string()),
                ("false".to_string(), "".to_string())
            ]
        );
    }

    #[test]
    fn lookup_local_reads_user_answer() {
        let bank = json!({
            "answer": { "data": { "problems": [
                problem(101, 1, "单选题", json!(["A"])),
                problem(102, 2, "多选题", json!(["B","C","D"])),
                problem(103, 6, "判断题", json!(["false"]))
            ]}},
            "info": {}
        });
        assert_eq!(lookup_local("101", &bank), Some(json!(["A"])));
        assert_eq!(lookup_local("102", &bank), Some(json!(["B", "C", "D"])));
        assert_eq!(lookup_local("103", &bank), Some(json!(["false"])));
        assert_eq!(lookup_local("999", &bank), None);
    }

    #[test]
    fn lookup_local_empty_answer_returns_none() {
        let bank = json!({
            "answer": { "data": { "problems": [
                problem(200, 1, "单选题", json!([]))
            ]}}
        });
        assert_eq!(lookup_local("200", &bank), None);
    }

    #[test]
    fn normalize_string_answer_splits_and_dedups() {
        // 字符串形态的答案（兼容其它来源）
        let bank = json!({"answer":{"data":{"problems":[
            {"problem_id":1,"user":{"answer":"A;B;A"}}
        ]}}});
        assert_eq!(lookup_local("1", &bank), Some(json!(["A", "B"])));
    }

    #[test]
    fn encode_single_to_array() {
        assert_eq!(
            encode_ai_answer("答案是 B", ProblemType::SingleChoice).unwrap(),
            json!(["B"])
        );
    }

    #[test]
    fn encode_single_ignores_answer_prefix_with_options() {
        let options = vec![
            ("A".to_string(), "甲".to_string()),
            ("B".to_string(), "乙".to_string()),
            ("C".to_string(), "丙".to_string()),
            ("D".to_string(), "丁".to_string()),
        ];
        assert_eq!(
            encode_ai_answer_with_options("Answer: B", ProblemType::SingleChoice, &options)
                .unwrap(),
            json!(["B"])
        );
        assert_eq!(
            encode_ai_answer_with_options(
                "The correct answer is C.",
                ProblemType::SingleChoice,
                &options,
            )
            .unwrap(),
            json!(["C"])
        );
    }

    #[test]
    fn encode_single_rejects_ambiguous_or_invalid_choice() {
        let options = vec![
            ("A".to_string(), "甲".to_string()),
            ("B".to_string(), "乙".to_string()),
            ("C".to_string(), "丙".to_string()),
            ("D".to_string(), "丁".to_string()),
        ];
        assert!(
            encode_ai_answer_with_options("A 或 B", ProblemType::SingleChoice, &options,).is_err()
        );
        assert!(
            encode_ai_answer_with_options("Answer: E", ProblemType::SingleChoice, &options,)
                .is_err()
        );
    }

    #[test]
    fn encode_multi_sorted_dedup_array() {
        assert_eq!(
            encode_ai_answer("C;A;A;D", ProblemType::MultiChoice).unwrap(),
            json!(["A", "C", "D"])
        );
    }

    #[test]
    fn encode_multi_ignores_english_words_with_options() {
        let options = vec![
            ("A".to_string(), "甲".to_string()),
            ("B".to_string(), "乙".to_string()),
            ("C".to_string(), "丙".to_string()),
            ("D".to_string(), "丁".to_string()),
        ];
        assert_eq!(
            encode_ai_answer_with_options("Answer: A;C", ProblemType::MultiChoice, &options)
                .unwrap(),
            json!(["A", "C"])
        );
        assert_eq!(
            encode_ai_answer_with_options("ACD", ProblemType::MultiChoice, &options).unwrap(),
            json!(["A", "C", "D"])
        );
    }

    #[test]
    fn encode_judge_semantics() {
        assert_eq!(
            encode_ai_answer("正确", ProblemType::Judge).unwrap(),
            json!(["true"])
        );
        assert_eq!(
            encode_ai_answer("错误", ProblemType::Judge).unwrap(),
            json!(["false"])
        );
        // 字母兜底：A=对、B=错
        assert_eq!(
            encode_ai_answer("A", ProblemType::Judge).unwrap(),
            json!(["true"])
        );
        assert_eq!(
            encode_ai_answer("B", ProblemType::Judge).unwrap(),
            json!(["false"])
        );
        // 解析不出 → Err
        assert!(encode_ai_answer("???", ProblemType::Judge).is_err());
    }

    #[test]
    fn encode_fill_blank_lines() {
        let v = encode_ai_answer("1=北京\n2=上海", ProblemType::FillBlank).unwrap();
        assert_eq!(v, json!({"1":["北京"], "2":["上海"]}));
    }

    #[test]
    fn encode_fill_blank_single_no_index() {
        let v = encode_ai_answer("地球", ProblemType::FillBlank).unwrap();
        assert_eq!(v, json!({"1":["地球"]}));
    }

    #[test]
    fn encode_fill_blank_rejects_refusal() {
        // AI 软失败：HTTP 成功但内容是拒答 / 无法作答 → Err（上层跳过，不提交）
        assert!(encode_ai_answer("抱歉，我无法回答这道题。", ProblemType::FillBlank).is_err());
        assert!(encode_ai_answer("无法确定答案", ProblemType::FillBlank).is_err());
        assert!(
            encode_ai_answer("I'm sorry, I cannot answer this question.", ProblemType::FillBlank)
                .is_err()
        );
        assert!(
            encode_ai_answer("There is not enough information.", ProblemType::FillBlank).is_err()
        );
    }

    #[test]
    fn encode_fill_blank_rejects_overlong_answer() {
        // 单空超长（疑似解释 / 跑题）→ Err
        let long = "字".repeat(MAX_BLANK_ANSWER_CHARS + 1);
        assert!(encode_ai_answer(&long, ProblemType::FillBlank).is_err());
    }

    #[test]
    fn encode_fill_blank_skips_invalid_keeps_valid() {
        // 多空中某空为拒答措辞被剔除，有效空仍保留
        let v = encode_ai_answer("1=北京\n2=抱歉无法确定", ProblemType::FillBlank).unwrap();
        assert_eq!(v, json!({"1":["北京"]}));
    }

    #[test]
    fn encode_fill_blank_all_invalid_returns_err() {
        // 全部空均为拒答 → 无有效空 → Err
        assert!(encode_ai_answer("1=无法确定\n2=不知道", ProblemType::FillBlank).is_err());
    }

    #[test]
    fn strip_html_works() {
        assert_eq!(strip_html("<p>你好&nbsp;世界</p>"), "你好 世界");
    }

    #[test]
    fn strip_html_decodes_encrypted_spans_via_decoder() {
        // 混淆码点 → 真字由运行时 decoder 提供（此处用固定映射模拟）。
        let decoder = FontDecodeMap::from_pairs(&[
            ('\u{8BCA}', '电'),
            ('\u{5C81}', '影'),
            ('\u{5145}', '和'),
            ('\u{8272}', '视'),
        ]);
        let html = "<p><span class=\"xuetangx-com-encrypted-font\">\u{8BCA}\u{5C81}\u{5145}\u{8BCA}\u{8272}</span></p>";
        assert_eq!(strip_html_with_decoder(html, Some(&decoder)), "电影和电视");
    }

    #[test]
    fn unknown_chars_when_decoder_missing_or_incomplete() {
        let q = json!({"content":{"Body":"<span class=\"xuetangx-com-encrypted-font\">\u{8BCA}\u{5C81}</span>"}});
        assert!(question_uses_encrypted_font(&q));
        // 无解码器：span 内疑似混淆字符全部视为未知。
        assert_eq!(
            encrypted_font_unknown_chars_with_decoder(&q, None),
            vec!['\u{8BCA}', '\u{5C81}']
        );
        // 解码器覆盖后无未知字符。
        let decoder = FontDecodeMap::from_pairs(&[('\u{8BCA}', '电'), ('\u{5C81}', '影')]);
        assert!(encrypted_font_unknown_chars_with_decoder(&q, Some(&decoder)).is_empty());
    }

    #[test]
    fn strip_html_does_not_decode_unmarked_text() {
        // 未标记加密字体的普通文本必须保持原文。
        assert_eq!(strip_html("<p>方法</p>"), "方法");
    }

    #[test]
    fn strip_html_normalizes_common_radicals() {
        // span 外的康熙部首兼容字（题目原文）做规范化，与加密字体无关。
        assert_eq!(strip_html("<p>\u{2F0F}\u{2F45}\u{2FAF}</p>"), "几方面");
    }

    #[test]
    fn summarize_quiz_sums_scores() {
        // 题3 user 无 my_score → 计满分不计得分/已答
        let bank = json!({
            "answer": {"data": {"problems": [
                {"problem_id":1, "content":{"Score":2.0}, "user":{"my_score":"2.00"}},
                {"problem_id":2, "content":{"Score":1.0}, "user":{"my_score":"0.00"}},
                {"problem_id":3, "content":{"Score":1.0}, "user":{}}
            ]}},
            "info": {}
        });
        let (score, total, answered, count) = summarize_quiz(&bank);
        assert_eq!(score, 2.0);
        assert_eq!(total, 4.0);
        assert_eq!(answered, 2);
        assert_eq!(count, 3);
    }

    #[test]
    fn summarize_quiz_empty_when_no_problems() {
        assert_eq!(
            summarize_quiz(&json!({"answer":{"data":{}}})),
            (0.0, 0.0, 0, 0)
        );
    }
}
