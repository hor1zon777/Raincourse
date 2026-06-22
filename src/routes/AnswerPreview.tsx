import { useCallback, useEffect, useMemo, useState } from 'react';
import { Alert, Button, Empty, List, Space, Spin, Tag, Typography, message, theme } from 'antd';
import { ArrowLeftOutlined, FileTextOutlined, ReloadOutlined } from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import { useNavigate, useParams } from 'react-router-dom';
import type { AnswerFile } from '../types';
import { normalizeError } from '../utils/errors';
import PageHeader from '../components/PageHeader';

interface PreviewOption {
  label: string;
  text: string;
  correct: boolean;
}

interface PreviewQuestion {
  key: string;
  index: number;
  problemId?: string;
  type?: string;
  body: string;
  answer: string;
  answerValues: string[];
  options: PreviewOption[];
  remark: string;
}

interface AnswerPreviewData {
  info: Partial<AnswerFile>;
  questions: PreviewQuestion[];
}

interface ChartRow {
  label: string;
  count: number;
  percent: number;
  color: string;
}

interface AnswerPreviewStats {
  total: number;
  answered: number;
  unanswered: number;
  answerRate: number;
  typeRows: ChartRow[];
  answerRows: ChartRow[];
}

const PROBLEM_PATHS = [
  ['data', 'problems'],
  ['data', 'exercises'],
  ['data', 'problem_list'],
  ['problems'],
];

const RESULT_PATHS = [
  ['exam', 'data', 'problem_results'],
  ['data', 'problem_results'],
  ['problem_results'],
  ['data', 'results'],
  ['results'],
];

const NESTED_QUESTION_KEYS = ['content', 'problem', 'question', 'problem_info', 'problem_result'];
const ID_KEYS = ['problem_id', 'ProblemID', 'problemId', 'id'];
const TYPE_KEYS = ['TypeText', 'type_text', 'Type', 'ProblemType', 'problem_type', 'type'];
const BODY_KEYS = ['Body', 'body', 'stem', 'title', 'name', 'text', 'content'];
const REMARK_KEYS = ['Remark', 'remark', 'analysis', 'explain', 'explanation'];
const OPTION_LIST_KEYS = ['Options', 'options'];
const OPTION_LABEL_KEYS = ['key', 'label', 'Key', 'index'];
const OPTION_TEXT_KEYS = ['value', 'content', 'text', 'option', 'body', 'title'];
const ANSWER_KEYS = [
  'answer',
  'Answer',
  'correct_answer',
  'standard_answer',
  'std_answer',
  'answers',
];

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const toDisplayString = (value: unknown): string => {
  if (value === null || value === undefined) return '';
  if (typeof value === 'string') return value.trim();
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return JSON.stringify(value) ?? '';
};

const stripHtml = (value: unknown): string =>
  toDisplayString(value)
    .replace(/<br\s*\/?>/gi, ' ')
    .replace(/<\/(p|div|li|tr|h[1-6])>/gi, ' ')
    .replace(/<[^>]+>/g, ' ')
    .replace(/&nbsp;/gi, ' ')
    .replace(/&lt;/gi, '<')
    .replace(/&gt;/gi, '>')
    .replace(/&amp;/gi, '&')
    .replace(/&quot;/gi, '"')
    .replace(/&#39;/gi, "'")
    .replace(/&#(\d+);/g, (_match, code: string) => String.fromCodePoint(Number(code)))
    .replace(/&#x([0-9a-f]+);/gi, (_match, code: string) =>
      String.fromCodePoint(Number.parseInt(code, 16)),
    )
    .replace(/\s+/g, ' ')
    .trim();

const valueAtPath = (source: unknown, path: string[]): unknown => {
  let current = source;
  for (const segment of path) {
    const record = isRecord(current) ? current : null;
    if (!record || !(segment in record)) return undefined;
    current = record[segment];
  }
  return current;
};

const firstArrayAtPaths = (source: unknown, paths: string[][]): unknown[] | null => {
  for (const path of paths) {
    const value = valueAtPath(source, path);
    if (Array.isArray(value)) return value;
  }
  return Array.isArray(source) ? source : null;
};

const collectQuestionObjects = (
  source: unknown,
  depth = 0,
  out: Record<string, unknown>[] = [],
): Record<string, unknown>[] => {
  const record = isRecord(source) ? source : null;
  if (!record || out.includes(record)) return out;
  out.push(record);
  if (depth >= 2) return out;
  for (const key of NESTED_QUESTION_KEYS) {
    collectQuestionObjects(record[key], depth + 1, out);
  }
  return out;
};

const firstScalarValue = (objects: Record<string, unknown>[], keys: string[]): unknown => {
  for (const object of objects) {
    for (const key of keys) {
      const value = object[key];
      if (
        value !== null &&
        value !== undefined &&
        (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean')
      ) {
        return value;
      }
    }
  }
  return undefined;
};

const firstValue = (objects: Record<string, unknown>[], keys: string[]): unknown => {
  for (const object of objects) {
    for (const key of keys) {
      const value = object[key];
      if (value !== null && value !== undefined) return value;
    }
  }
  return undefined;
};

const formatProblemType = (value: unknown): string | undefined => {
  const raw = toDisplayString(value);
  const lowered = raw.toLowerCase();
  if (!raw) return undefined;
  if (raw === '1' || raw.includes('单选') || lowered === 'singlechoice') return '单选题';
  if (raw === '2' || raw.includes('多选') || lowered === 'multiplechoice') return '多选题';
  if (raw === '3' || raw === '6' || raw.includes('判断') || lowered === 'judgement' || lowered === 'judgment') {
    return '判断题';
  }
  if (raw === '4' || raw.includes('填空') || lowered === 'blankfilling' || lowered === 'fillblank') {
    return '填空题';
  }
  return raw;
};

const letterAt = (index: number): string => {
  const code = 'A'.charCodeAt(0) + index;
  return code <= 'Z'.charCodeAt(0) ? String.fromCharCode(code) : String(index + 1);
};

const judgeCanonicalValue = (value: unknown): 'true' | 'false' | null => {
  const raw = toDisplayString(value).toLowerCase();
  if (['true', 'a', '正确', '对', '是', 'yes'].includes(raw)) return 'true';
  if (['false', 'b', '错误', '错', '否', 'no'].includes(raw)) return 'false';
  return null;
};

const formatJudgeValue = (value: unknown): string | null => {
  const canonical = judgeCanonicalValue(value);
  if (canonical === 'true') return '√';
  if (canonical === 'false') return '×';
  return null;
};

const optionCompareKey = (value: unknown, type?: string): string => {
  if (type === '判断题') {
    const judge = judgeCanonicalValue(value);
    if (judge) return judge;
  }
  return toDisplayString(value).toLowerCase();
};

const splitAnswerString = (value: string, type?: string): string[] => {
  const trimmed = value.trim();
  if (!trimmed) return [];
  const isChoice = type === '单选题' || type === '多选题' || type === '判断题';
  if (!isChoice) return [trimmed];

  const separated = trimmed.split(/[;,，、\s]+/).filter(Boolean);
  if (separated.length > 1) return separated;
  if (type === '多选题' && /^[A-Za-z]{2,}$/.test(trimmed)) return trimmed.toUpperCase().split('');
  return [trimmed];
};

const normalizeAnswerValues = (raw: unknown, type?: string): string[] => {
  if (raw === null || raw === undefined) return [];
  if (Array.isArray(raw)) {
    return raw.map(toDisplayString).filter(Boolean);
  }
  if (isRecord(raw)) {
    return Object.entries(raw)
      .sort(([a], [b]) => Number(a) - Number(b))
      .flatMap(([, value]) => (Array.isArray(value) ? value : [value]))
      .map(toDisplayString)
      .filter(Boolean);
  }
  return splitAnswerString(toDisplayString(raw), type);
};

const formatBlankAnswer = (raw: Record<string, unknown>): string => {
  const entries = Object.entries(raw).sort(([a], [b]) => Number(a) - Number(b));
  if (entries.length === 0) return '答案缺失';
  return entries
    .map(([key, value]) => {
      const values = (Array.isArray(value) ? value : [value]).map(toDisplayString).filter(Boolean);
      return `第${key}空：${values.join(' / ') || '空'}`;
    })
    .join('；');
};

const formatAnswerDisplay = (raw: unknown, type: string | undefined, options: PreviewOption[]): string => {
  if (raw === null || raw === undefined) return '答案未获取';
  if (isRecord(raw)) return formatBlankAnswer(raw);

  const values = normalizeAnswerValues(raw, type);
  if (values.length === 0) return '答案缺失';
  if (type === '判断题') {
    return values.map((value) => formatJudgeValue(value) ?? toDisplayString(value)).join('、');
  }

  const matchedOptions = values
    .map((value) =>
      options.find((option) => optionCompareKey(option.label, type) === optionCompareKey(value, type)),
    )
    .filter((option): option is PreviewOption => Boolean(option));
  if (matchedOptions.length > 0) {
    return matchedOptions
      .map((option) => `${option.label}${option.text ? `：${option.text}` : ''}`)
      .join('；');
  }
  return values.join('、');
};

const extractOptions = (objects: Record<string, unknown>[], type?: string): PreviewOption[] => {
  const rawOptions = firstValue(objects, OPTION_LIST_KEYS);
  if (!Array.isArray(rawOptions)) return [];

  return rawOptions.map((option, index) => {
    if (!isRecord(option)) {
      return {
        label: letterAt(index),
        text: stripHtml(option),
        correct: false,
      };
    }

    const optionObjects = collectQuestionObjects(option);
    const label = toDisplayString(firstScalarValue(optionObjects, OPTION_LABEL_KEYS)) || letterAt(index);
    const text =
      stripHtml(firstScalarValue(optionObjects, OPTION_TEXT_KEYS)) ||
      (type === '判断题' ? formatJudgeValue(label) ?? '' : '');
    return { label, text, correct: false };
  });
};

const extractAnswerRaw = (objects: Record<string, unknown>[]): unknown => {
  const answerObjects: Record<string, unknown>[] = [];
  for (const object of objects) {
    const user = object.user;
    if (isRecord(user)) answerObjects.push(user);
    answerObjects.push(object);
  }
  return firstValue(answerObjects, ANSWER_KEYS);
};

const buildQuestionPreview = (source: unknown, index: number): PreviewQuestion => {
  const objects = collectQuestionObjects(source);
  const problemId = toDisplayString(firstScalarValue(objects, ID_KEYS)) || undefined;
  const type = formatProblemType(firstScalarValue(objects, TYPE_KEYS));
  const body = stripHtml(firstScalarValue(objects, BODY_KEYS));
  const remark = stripHtml(firstScalarValue(objects, REMARK_KEYS));
  const rawAnswer = extractAnswerRaw(objects);
  const answerValues = normalizeAnswerValues(rawAnswer, type);
  const correctKeys = new Set(answerValues.map((value) => optionCompareKey(value, type)));
  const options = extractOptions(objects, type).map((option) => ({
    ...option,
    correct: correctKeys.has(optionCompareKey(option.label, type)),
  }));

  return {
    key: problemId || String(index + 1),
    index: index + 1,
    problemId,
    type,
    body,
    answer: formatAnswerDisplay(rawAnswer, type, options),
    answerValues,
    options,
    remark,
  };
};

const buildAnswerPreview = (content: unknown, fileName: string): AnswerPreviewData => {
  const contentRecord = isRecord(content) ? content : null;
  const rawInfo = contentRecord && isRecord(contentRecord.info) ? contentRecord.info : {};
  const info = { file_name: fileName, ...rawInfo } as Partial<AnswerFile>;
  const answerRoot = contentRecord && 'answer' in contentRecord ? contentRecord.answer : content;

  const problemItems = firstArrayAtPaths(answerRoot, PROBLEM_PATHS);
  const resultItems = problemItems ? null : firstArrayAtPaths(answerRoot, RESULT_PATHS);
  const items = problemItems ?? resultItems ?? [];

  return {
    info,
    questions: items.map((item, index) => buildQuestionPreview(item, index)),
  };
};

const answerPreviewTitle = (preview: AnswerPreviewData | null): string =>
  preview?.info.exam_name || '答案预览';

const chartColors = ['#1677ff', '#52c41a', '#fa8c16', '#eb2f96', '#13c2c2', '#722ed1', '#8c8c8c'];

const percentOf = (count: number, total: number): number =>
  total > 0 ? Math.round((count / total) * 100) : 0;

const buildChartRows = (counts: Map<string, number>, total: number): ChartRow[] =>
  Array.from(counts.entries())
    .sort(([aLabel, aCount], [bLabel, bCount]) => bCount - aCount || aLabel.localeCompare(bLabel))
    .map(([label, count], index) => ({
      label,
      count,
      percent: percentOf(count, total),
      color: chartColors[index % chartColors.length],
    }));

const answerChartLabel = (question: PreviewQuestion, rawValue: string): string => {
  if (question.type === '判断题') return formatJudgeValue(rawValue) ?? rawValue;
  return rawValue.toUpperCase();
};

const extractAnswerChartLabels = (question: PreviewQuestion): string[] => {
  if (!['单选题', '多选题', '判断题'].includes(question.type ?? '')) return [];
  const optionLabels = question.options.filter((option) => option.correct).map((option) => option.label);
  const rawLabels = optionLabels.length > 0 ? optionLabels : question.answerValues;
  return rawLabels.map((value) => answerChartLabel(question, value)).filter(Boolean);
};

const buildPreviewStats = (questions: PreviewQuestion[]): AnswerPreviewStats => {
  const total = questions.length;
  const typeCounts = new Map<string, number>();
  const answerCounts = new Map<string, number>();
  let answered = 0;

  for (const question of questions) {
    const type = question.type || '未知题型';
    typeCounts.set(type, (typeCounts.get(type) ?? 0) + 1);
    if (question.answerValues.length > 0) answered += 1;
    for (const label of extractAnswerChartLabels(question)) {
      answerCounts.set(label, (answerCounts.get(label) ?? 0) + 1);
    }
  }

  return {
    total,
    answered,
    unanswered: total - answered,
    answerRate: percentOf(answered, total),
    typeRows: buildChartRows(typeCounts, total),
    answerRows: buildChartRows(answerCounts, Math.max(Array.from(answerCounts.values()).reduce((sum, n) => sum + n, 0), 0)),
  };
};

function ChartRows({ rows }: { rows: ChartRow[] }) {
  if (rows.length === 0) {
    return <Typography.Text type="secondary">暂无可统计数据</Typography.Text>;
  }
  return (
    <Space direction="vertical" size={8} style={{ width: '100%' }}>
      {rows.map((row) => (
        <div
          key={row.label}
          style={{
            display: 'grid',
            gridTemplateColumns: '84px minmax(140px, 1fr) 72px',
            alignItems: 'center',
            gap: 10,
            minHeight: 28,
          }}
        >
          <Typography.Text ellipsis title={row.label}>
            {row.label}
          </Typography.Text>
          <div
            style={{
              height: 10,
              borderRadius: 5,
              background: '#f0f0f0',
              overflow: 'hidden',
            }}
          >
            <div
              style={{
                width: `${Math.max(row.percent, row.count > 0 ? 3 : 0)}%`,
                height: '100%',
                borderRadius: 5,
                background: row.color,
              }}
            />
          </div>
          <Typography.Text type="secondary" style={{ textAlign: 'right', fontVariantNumeric: 'tabular-nums' }}>
            {row.count} ({row.percent}%)
          </Typography.Text>
        </div>
      ))}
    </Space>
  );
}

function AnswerCharts({ stats }: { stats: AnswerPreviewStats }) {
  const { token } = theme.useToken();
  const metricStyle = {
    padding: '12px 14px',
    border: `1px solid ${token.colorBorderSecondary}`,
    borderRadius: 6,
    background: token.colorFillQuaternary,
    minHeight: 78,
  };
  const sectionStyle = {
    padding: 16,
    border: `1px solid ${token.colorBorderSecondary}`,
    borderRadius: 6,
    background: token.colorBgContainer,
  };

  return (
    <Space direction="vertical" size={12} style={{ width: '100%' }}>
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(140px, 1fr))',
          gap: 12,
        }}
      >
        <div style={metricStyle}>
          <Typography.Text type="secondary">题目总数</Typography.Text>
          <div style={{ fontSize: 28, fontWeight: 700, lineHeight: 1.25 }}>{stats.total}</div>
        </div>
        <div style={metricStyle}>
          <Typography.Text type="secondary">已获取答案</Typography.Text>
          <div style={{ fontSize: 28, fontWeight: 700, lineHeight: 1.25, color: token.colorSuccess }}>
            {stats.answered}
          </div>
        </div>
        <div style={metricStyle}>
          <Typography.Text type="secondary">缺失答案</Typography.Text>
          <div style={{ fontSize: 28, fontWeight: 700, lineHeight: 1.25, color: token.colorWarning }}>
            {stats.unanswered}
          </div>
        </div>
        <div style={metricStyle}>
          <Typography.Text type="secondary">答案覆盖率</Typography.Text>
          <div style={{ fontSize: 28, fontWeight: 700, lineHeight: 1.25, color: token.colorPrimary }}>
            {stats.answerRate}%
          </div>
        </div>
      </div>

      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))',
          gap: 12,
        }}
      >
        <div style={sectionStyle}>
          <Typography.Text strong>题型分布</Typography.Text>
          <div style={{ marginTop: 12 }}>
            <ChartRows rows={stats.typeRows} />
          </div>
        </div>
        <div style={sectionStyle}>
          <Typography.Text strong>选择/判断答案分布</Typography.Text>
          <div style={{ marginTop: 12 }}>
            <ChartRows rows={stats.answerRows} />
          </div>
        </div>
      </div>
    </Space>
  );
}

function AnswerNumberGrid({ questions }: { questions: PreviewQuestion[] }) {
  const { token } = theme.useToken();

  return (
    <div
      style={{
        padding: 16,
        border: `1px solid ${token.colorBorderSecondary}`,
        borderRadius: 6,
        background: token.colorBgContainer,
      }}
    >
      <Typography.Text strong>答案速览</Typography.Text>
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(118px, 1fr))',
          gap: 10,
          marginTop: 12,
        }}
      >
        {questions.map((question) => (
          <div
            key={`answer-box-${question.key}`}
            title={`第 ${question.index} 题：${question.answer}`}
            style={{
              minHeight: 58,
              padding: '9px 10px',
              border: `1px solid ${question.answerValues.length > 0 ? '#b7eb8f' : token.colorBorderSecondary}`,
              borderRadius: 6,
              background: question.answerValues.length > 0 ? '#f6ffed' : token.colorFillQuaternary,
              overflow: 'hidden',
            }}
          >
            <Typography.Text
              strong
              style={{
                display: 'block',
                color: question.answerValues.length > 0 ? '#237804' : token.colorTextSecondary,
                fontVariantNumeric: 'tabular-nums',
              }}
            >
              第 {question.index} 题
            </Typography.Text>
            <Typography.Text
              style={{
                display: 'block',
                marginTop: 4,
                maxWidth: '100%',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
                color: token.colorText,
              }}
            >
              {question.answer}
            </Typography.Text>
          </div>
        ))}
      </div>
    </div>
  );
}

export default function AnswerPreview() {
  const navigate = useNavigate();
  const params = useParams<{ fileName: string }>();
  const fileName = useMemo(() => {
    const raw = params.fileName ?? '';
    try {
      return decodeURIComponent(raw);
    } catch {
      return raw;
    }
  }, [params.fileName]);
  const [preview, setPreview] = useState<AnswerPreviewData | null>(null);
  const [loading, setLoading] = useState(false);
  const stats = useMemo(
    () => (preview ? buildPreviewStats(preview.questions) : null),
    [preview],
  );

  const loadPreview = useCallback(async () => {
    if (!fileName) {
      setPreview(null);
      return;
    }
    setLoading(true);
    try {
      const content = await invoke<unknown>('get_answer_file_content', { fileName });
      setPreview(buildAnswerPreview(content, fileName));
    } catch (e) {
      setPreview(null);
      message.error(`读取答案文件失败: ${normalizeError(e).message}`);
    } finally {
      setLoading(false);
    }
  }, [fileName]);

  useEffect(() => {
    loadPreview();
  }, [loadPreview]);

  return (
    <div>
      <PageHeader
        before={
          <Button icon={<ArrowLeftOutlined />} onClick={() => navigate('/answers')}>
            返回
          </Button>
        }
        icon={<FileTextOutlined />}
        title={answerPreviewTitle(preview)}
        extra={
          <Button icon={<ReloadOutlined />} onClick={loadPreview} loading={loading}>
            刷新
          </Button>
        }
      />

      <Spin spinning={loading}>
        {!fileName ? (
          <Alert type="warning" showIcon message="缺少答案文件名" />
        ) : preview ? (
          <Space direction="vertical" size={16} style={{ width: '100%' }}>
            <Space size="small" wrap>
              {preview.info.course_name && <Tag color="blue">{preview.info.course_name}</Tag>}
              {preview.info.exam_type && <Tag>{preview.info.exam_type}</Tag>}
              {preview.info.exam_id && <Tag>ID {preview.info.exam_id}</Tag>}
              <Tag color="green">共 {preview.questions.length} 题</Tag>
            </Space>

            {preview.questions.length === 0 ? (
              <Empty description="未能解析出题目答案" />
            ) : (
              <>
                {stats && <AnswerCharts stats={stats} />}
                <AnswerNumberGrid questions={preview.questions} />
                <List
                  itemLayout="vertical"
                  dataSource={preview.questions}
                  renderItem={(question) => (
                    <List.Item key={question.key}>
                      <Space direction="vertical" size={10} style={{ width: '100%' }}>
                        <Space size="small" wrap>
                          <Tag color="processing">第 {question.index} 题</Tag>
                          {question.type && <Tag>{question.type}</Tag>}
                          {question.problemId && (
                            <Typography.Text type="secondary" code>
                              {question.problemId}
                            </Typography.Text>
                          )}
                        </Space>

                        <div
                          style={{
                            padding: '10px 12px',
                            border: '1px solid #b7eb8f',
                            borderRadius: 6,
                            background: '#f6ffed',
                          }}
                        >
                          <Typography.Text strong>正确答案：</Typography.Text>
                          <Typography.Text strong style={{ color: '#237804' }}>
                            {question.answer}
                          </Typography.Text>
                        </div>

                        {question.body && (
                          <Typography.Paragraph style={{ marginBottom: 0 }}>
                            {question.body}
                          </Typography.Paragraph>
                        )}

                        {question.options.length > 0 && (
                          <Space direction="vertical" size={6} style={{ width: '100%' }}>
                            {question.options.map((option) => (
                              <div
                                key={`${question.key}-${option.label}`}
                                style={{
                                  display: 'flex',
                                  gap: 8,
                                  alignItems: 'flex-start',
                                  padding: '7px 9px',
                                  border: option.correct ? '1px solid #b7eb8f' : '1px solid #f0f0f0',
                                  borderRadius: 6,
                                  background: option.correct ? '#f6ffed' : '#fff',
                                }}
                              >
                                <Tag color={option.correct ? 'green' : undefined} style={{ marginInlineEnd: 0 }}>
                                  {option.label}
                                </Tag>
                                <Typography.Text>{option.text || '-'}</Typography.Text>
                              </div>
                            ))}
                          </Space>
                        )}

                        {question.remark && (
                          <Typography.Text type="secondary">解析：{question.remark}</Typography.Text>
                        )}

                        {question.answerValues.length === 0 && (
                          <Typography.Text type="secondary">该题未在文件中找到可展示的答案值。</Typography.Text>
                        )}
                      </Space>
                    </List.Item>
                  )}
                />
              </>
            )}
          </Space>
        ) : (
          <Empty description="暂无预览内容" />
        )}
      </Spin>
    </div>
  );
}
