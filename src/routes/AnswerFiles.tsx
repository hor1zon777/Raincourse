import { useEffect, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Drawer,
  Empty,
  List,
  Popconfirm,
  Space,
  Spin,
  Table,
  Tag,
  Typography,
  message,
} from 'antd';
import { DeleteOutlined, EyeOutlined, FileTextOutlined, ReloadOutlined } from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import type { AnswerFile } from '../types';
import { normalizeError } from '../utils/errors';
import PageHeader from '../components/PageHeader';

interface DeleteBatchResult {
  deleted: number;
  failed: { file_name: string; reason: string }[];
}

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

interface AnswerPreview {
  info: Partial<AnswerFile>;
  questions: PreviewQuestion[];
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
    .replace(/&#(\d+);/g, (_match, code: string) => String.fromCharCode(Number(code)))
    .replace(/&#x([0-9a-f]+);/gi, (_match, code: string) =>
      String.fromCharCode(Number.parseInt(code, 16)),
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
  if (!raw) return undefined;
  if (raw === '1' || raw.includes('单选') || raw.toLowerCase() === 'singlechoice') return '单选题';
  if (raw === '2' || raw.includes('多选') || raw.toLowerCase() === 'multiplechoice') return '多选题';
  if (
    raw === '3' ||
    raw === '6' ||
    raw.includes('判断') ||
    raw.toLowerCase() === 'judgement' ||
    raw.toLowerCase() === 'judgment'
  ) {
    return '判断题';
  }
  if (
    raw === '4' ||
    raw.includes('填空') ||
    raw.toLowerCase() === 'blankfilling' ||
    raw.toLowerCase() === 'fillblank'
  ) {
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
  if (canonical === 'true') return '正确';
  if (canonical === 'false') return '错误';
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
    .map((value) => options.find((option) => optionCompareKey(option.label, type) === optionCompareKey(value, type)))
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

const buildAnswerPreview = (content: unknown, fallback: AnswerFile): AnswerPreview => {
  const contentRecord = isRecord(content) ? content : null;
  const rawInfo = contentRecord && isRecord(contentRecord.info) ? contentRecord.info : {};
  const info = { ...fallback, ...rawInfo } as Partial<AnswerFile>;
  const answerRoot = contentRecord && 'answer' in contentRecord ? contentRecord.answer : content;

  const problemItems = firstArrayAtPaths(answerRoot, PROBLEM_PATHS);
  const resultItems = problemItems ? null : firstArrayAtPaths(answerRoot, RESULT_PATHS);
  const items = problemItems ?? resultItems ?? [];

  return {
    info,
    questions: items.map((item, index) => buildQuestionPreview(item, index)),
  };
};

const previewTitle = (preview: AnswerPreview | null, record: AnswerFile | null): string =>
  preview?.info.exam_name || record?.exam_name || '答案预览';

export default function AnswerFiles() {
  const [files, setFiles] = useState<AnswerFile[]>([]);
  const [loading, setLoading] = useState(false);
  // 选中的文件（以真实 file_name 为键）
  const [selectedKeys, setSelectedKeys] = useState<string[]>([]);
  // 批量删除进行中
  const [batchDeleting, setBatchDeleting] = useState(false);
  // 单个删除进行中的 file_name
  const [deletingOne, setDeletingOne] = useState<string | null>(null);
  const [previewOpen, setPreviewOpen] = useState(false);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewRecord, setPreviewRecord] = useState<AnswerFile | null>(null);
  const [preview, setPreview] = useState<AnswerPreview | null>(null);

  const fetchFiles = async () => {
    setLoading(true);
    try {
      const data = await invoke<AnswerFile[]>('get_answer_files');
      setFiles(data);
      // 列表刷新后，剔除已不存在的选中项
      setSelectedKeys((prev) => {
        const existing = new Set(data.map((f) => f.file_name));
        return prev.filter((k) => existing.has(k));
      });
    } catch (e) {
      const err = normalizeError(e);
      // 目录不存在等可恢复错误降级为空列表，但仍提示用户
      if (err.code === 'IO_ERROR') {
        setFiles([]);
        message.info('暂无答案文件目录，请先在课程详情中导出答案');
      } else {
        message.error(`获取答案文件失败: ${err.message}`);
      }
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchFiles();
  }, []);

  const handlePreview = async (record: AnswerFile) => {
    setPreviewRecord(record);
    setPreview(null);
    setPreviewOpen(true);
    setPreviewLoading(true);
    try {
      const content = await invoke<unknown>('get_answer_file_content', { fileName: record.file_name });
      setPreview(buildAnswerPreview(content, record));
    } catch (e) {
      message.error(`读取答案文件失败: ${normalizeError(e).message}`);
    } finally {
      setPreviewLoading(false);
    }
  };

  // 删除单个答案文件
  const handleDeleteOne = async (record: AnswerFile) => {
    setDeletingOne(record.file_name);
    try {
      await invoke('delete_answer_file', { fileName: record.file_name });
      message.success(`已删除：${record.exam_name || record.file_name}`);
      if (previewRecord?.file_name === record.file_name) {
        setPreviewOpen(false);
        setPreview(null);
        setPreviewRecord(null);
      }
      await fetchFiles();
    } catch (e) {
      message.error(`删除失败: ${normalizeError(e).message}`);
    } finally {
      setDeletingOne(null);
    }
  };

  // 批量删除选中的答案文件
  const handleDeleteBatch = async () => {
    if (selectedKeys.length === 0) return;
    setBatchDeleting(true);
    try {
      const res = await invoke<DeleteBatchResult>('delete_answer_files', {
        fileNames: selectedKeys,
      });
      if (res.failed.length === 0) {
        message.success(`已删除 ${res.deleted} 个答案文件`);
      } else {
        message.warning(`删除完成：成功 ${res.deleted} 个，失败 ${res.failed.length} 个`);
      }
      if (previewRecord && selectedKeys.includes(previewRecord.file_name)) {
        setPreviewOpen(false);
        setPreview(null);
        setPreviewRecord(null);
      }
      setSelectedKeys([]);
      await fetchFiles();
    } catch (e) {
      message.error(`批量删除失败: ${normalizeError(e).message}`);
    } finally {
      setBatchDeleting(false);
    }
  };

  const columns = [
    {
      title: '序号',
      key: 'index',
      width: 60,
      render: (_: unknown, __: unknown, i: number) => i + 1,
    },
    {
      title: '考试 ID',
      dataIndex: 'exam_id',
      key: 'exam_id',
      width: 120,
    },
    {
      title: '课程',
      dataIndex: 'course_name',
      key: 'course_name',
      width: 180,
      render: (v: string | undefined) => v || '-',
    },
    {
      title: '名称',
      dataIndex: 'exam_name',
      key: 'exam_name',
    },
    {
      title: '类型',
      dataIndex: 'exam_type',
      key: 'exam_type',
      width: 100,
    },
    {
      title: '操作',
      key: 'action',
      width: 160,
      render: (_: unknown, record: AnswerFile) => (
        <Space size={0}>
          <Button
            type="link"
            size="small"
            icon={<EyeOutlined />}
            onClick={() => handlePreview(record)}
          >
            预览
          </Button>
          <Popconfirm
            title="确定删除该答案文件？"
            description="仅删除本地导出的答案文件，不影响雨课堂平台数据。"
            onConfirm={() => handleDeleteOne(record)}
            okText="确定删除"
            cancelText="取消"
            okButtonProps={{ danger: true }}
          >
            <Button
              type="link"
              size="small"
              danger
              icon={<DeleteOutlined />}
              loading={deletingOne === record.file_name}
            >
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <div>
      <PageHeader
        icon={<FileTextOutlined />}
        title="答案文件"
        extra={
          <Space>
            <Popconfirm
              title={`确定删除选中的 ${selectedKeys.length} 个答案文件？`}
              description="仅删除本地导出的答案文件，不影响雨课堂平台数据。"
              onConfirm={handleDeleteBatch}
              okText="确定删除"
              cancelText="取消"
              okButtonProps={{ danger: true }}
              disabled={selectedKeys.length === 0}
            >
              <Button
                danger
                icon={<DeleteOutlined />}
                disabled={selectedKeys.length === 0}
                loading={batchDeleting}
              >
                批量删除{selectedKeys.length > 0 ? ` (${selectedKeys.length})` : ''}
              </Button>
            </Popconfirm>
            <Button icon={<ReloadOutlined />} onClick={fetchFiles} loading={loading}>
              刷新
            </Button>
          </Space>
        }
      />

      <Card>
        <Spin spinning={loading}>
          {files.length === 0 ? (
            <Empty description="暂无答案文件" />
          ) : (
            <Table
              rowSelection={{
                selectedRowKeys: selectedKeys,
                onChange: (keys) => setSelectedKeys(keys as string[]),
              }}
              columns={columns}
              dataSource={files}
              rowKey="file_name"
              pagination={false}
              size="middle"
              scroll={{ x: 'max-content' }}
            />
          )}
        </Spin>
      </Card>

      <Drawer
        title="答案预览"
        width={720}
        open={previewOpen}
        onClose={() => setPreviewOpen(false)}
      >
        <Spin spinning={previewLoading}>
          {preview ? (
            <Space direction="vertical" size={16} style={{ width: '100%' }}>
              <div>
                <Typography.Title level={5} style={{ margin: 0 }}>
                  {previewTitle(preview, previewRecord)}
                </Typography.Title>
                <Space size="small" wrap style={{ marginTop: 8 }}>
                  {preview.info.course_name && <Tag color="blue">{preview.info.course_name}</Tag>}
                  {preview.info.exam_type && <Tag>{preview.info.exam_type}</Tag>}
                  {preview.info.exam_id && <Tag> ID {preview.info.exam_id}</Tag>}
                  <Tag color="green">共 {preview.questions.length} 题</Tag>
                </Space>
              </div>

              {preview.questions.length === 0 ? (
                <Empty description="未能解析出题目答案" />
              ) : (
                <>
                  <Alert
                    type="success"
                    showIcon
                    message="每题的正确答案已在题目顶部高亮显示。"
                  />
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
          ) : previewLoading ? null : (
            <Empty description="请选择一个答案文件预览" />
          )}
        </Spin>
      </Drawer>
    </div>
  );
}
