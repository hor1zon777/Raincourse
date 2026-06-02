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
//! 安全原则：任何字段提取 / 编码失败都返回 `None`/`Err`，由上层「跳过该题、不提交」，
//! 绝不用猜测出的脏数据发起提交。

use serde_json::{json, Value};

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
const TYPE_KEYS: &[&str] = &["TypeText", "Type", "ProblemType", "type_text", "problem_type", "type"];
/// 题干字段候选（位于 content 节点）。
const BODY_KEYS: &[&str] = &["Body", "body", "content", "stem", "title"];
/// 选项列表字段候选（位于 content 节点）。
const OPTION_LIST_KEYS: &[&str] = &["Options", "options"];
/// 选项文本字段候选。
const OPTION_TEXT_KEYS: &[&str] = &["value", "content", "text", "option", "body", "title"];
/// 选项标签字段候选（真实样本为 `key`：A/B/C/D 或 true/false）。
const OPTION_LABEL_KEYS: &[&str] = &["key", "label", "Key", "index"];
/// 题库里每题「正确答案」字段候选（题目顶层 / content 内的兜底；主路径见 `extract_correct_answer`）。
const ANSWER_KEYS: &[&str] = &["answer", "Answer", "correct_answer", "std_answer", "answers"];
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
            if s.contains("判断") || s.eq_ignore_ascii_case("Judgement") || s.eq_ignore_ascii_case("Judgment") {
                return Some(ProblemType::Judge);
            }
            if s.contains("填空") || s.eq_ignore_ascii_case("BlankFilling") || s.eq_ignore_ascii_case("FillBlank") {
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

/// 提取题干纯文本（去 HTML 标签）。
pub fn extract_body(q: &Value) -> String {
    let raw = pick_str(content_node(q), BODY_KEYS).unwrap_or_default();
    strip_html(&raw)
}

/// 提取选项列表 `[(label, text)]`。label 取选项的 `key`（真实即 A/B/C/D，
/// 判断题为 true/false）；缺失时按序补 A/B/C…。text 取选项文本字段并去 HTML。
pub fn extract_options(q: &Value) -> Vec<(String, String)> {
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
                out.push((label, strip_html(s)));
                continue;
            }
            let label = pick_str(opt, OPTION_LABEL_KEYS)
                .unwrap_or_else(|| ((b'A' + (i.min(25) as u8)) as char).to_string());
            let text = pick_str(opt, OPTION_TEXT_KEYS).unwrap_or_default();
            out.push((label, strip_html(&text)));
        }
    }
    out
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
pub fn encode_ai_answer(raw: &str, qtype: ProblemType) -> Result<Value, String> {
    let raw = raw.trim();
    match qtype {
        ProblemType::SingleChoice => {
            let letter = first_option_letter(raw)
                .ok_or_else(|| format!("无法从 AI 输出解析单选答案: {:?}", raw))?;
            Ok(json!([letter.to_string()]))
        }
        ProblemType::MultiChoice => {
            let letters = all_option_letters(raw);
            if letters.is_empty() {
                return Err(format!("无法从 AI 输出解析多选答案: {:?}", raw));
            }
            Ok(json!(letters
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()))
        }
        ProblemType::Judge => {
            let v = parse_judge(raw)
                .ok_or_else(|| format!("无法从 AI 输出解析判断答案: {:?}", raw))?;
            Ok(json!([v]))
        }
        ProblemType::FillBlank => {
            parse_fill_blanks(raw).ok_or_else(|| format!("无法从 AI 输出解析填空答案: {:?}", raw))
        }
    }
}

/// 取第一个 A-Z 选项字母。
fn first_option_letter(s: &str) -> Option<char> {
    s.chars()
        .find(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
}

/// 取所有去重升序的选项字母。
fn all_option_letters(s: &str) -> Vec<char> {
    let mut set = std::collections::BTreeSet::new();
    for c in s.chars() {
        if c.is_ascii_alphabetic() {
            set.insert(c.to_ascii_uppercase());
        }
    }
    set.into_iter().collect()
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

/// 解析填空：逐行 `序号=答案` → `{"1":["答案"]}`；无序号时整体作为第 1 空。
fn parse_fill_blanks(s: &str) -> Option<Value> {
    let mut map = serde_json::Map::new();
    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim();
            if !k.is_empty() && !v.is_empty() {
                map.insert(k.to_string(), json!([v]));
            }
        }
    }
    if map.is_empty() {
        let v = s.trim();
        if v.is_empty() {
            return None;
        }
        map.insert("1".to_string(), json!([v]));
    }
    Some(Value::Object(map))
}

/// 极简 HTML 清理：去标签 + 常见实体（够用即可）。
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .trim()
        .to_string()
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
        assert_eq!(ProblemType::from_question(&json!({"content":{"x":1}})), None);
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
    fn encode_multi_sorted_dedup_array() {
        assert_eq!(
            encode_ai_answer("C;A;A;D", ProblemType::MultiChoice).unwrap(),
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
        assert_eq!(encode_ai_answer("A", ProblemType::Judge).unwrap(), json!(["true"]));
        assert_eq!(encode_ai_answer("B", ProblemType::Judge).unwrap(), json!(["false"]));
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
    fn strip_html_works() {
        assert_eq!(strip_html("<p>你好&nbsp;世界</p>"), "你好 世界");
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
        assert_eq!(summarize_quiz(&json!({"answer":{"data":{}}})), (0.0, 0.0, 0, 0));
    }
}
