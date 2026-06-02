import { useCallback, useEffect, useRef, useState } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { Table, Tabs, Typography, Button, Space, Card, Spin, Tag, message, Modal, Select, Input, Alert, Drawer, Progress, List } from 'antd';
import {
  DownloadOutlined,
  PlayCircleOutlined,
  ExportOutlined,
  ArrowLeftOutlined,
  UnorderedListOutlined,
  ReloadOutlined,
  CheckSquareOutlined,
  FilterOutlined,
  RobotOutlined,
  ExperimentOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
} from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import { useCourseStore } from '../stores/courseStore';
import { normalizeError } from '../utils/errors';
import { useTauriListens } from '../utils/useTauriListens';
import type { Work, ExportResult, QuizAnswerEvent, QuizAnswerResult, LearnSchedule, QuizScore } from '../types';

const { Title } = Typography;

interface ChapterTask {
  index: number;
  id: number;
  name: string;
  leaf_type: number;
  type_str: string;
}

const LEAF_TYPE_OPTIONS = [
  { value: 0, label: '视频', color: 'blue' },
  { value: 3, label: '公告', color: 'cyan' },
  { value: 4, label: '讨论', color: 'purple' },
  { value: 6, label: '测验/练习', color: 'orange' },
];

export default function CourseDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  // 细粒度 selector
  const works = useCourseStore((s) => s.works);
  const ppts = useCourseStore((s) => s.ppts);
  const worksLoading = useCourseStore((s) => s.worksLoading);
  const pptsLoading = useCourseStore((s) => s.pptsLoading);
  const fetchWorks = useCourseStore((s) => s.fetchWorks);
  const fetchPpts = useCourseStore((s) => s.fetchPpts);
  const setCourseContext = useCourseStore((s) => s.setCourseContext);

  const [exporting, setExporting] = useState<number | null>(null);
  const [batchExporting, setBatchExporting] = useState(false);
  const [batchProgress, setBatchProgress] = useState<{ done: number; total: number } | null>(null);
  // 批量导出停止标志，兼作课程切换 / 卸载时中断正在进行的循环
  const batchStopRef = useRef(false);
  const [chapterTasks, setChapterTasks] = useState<ChapterTask[]>([]);
  const [chapterLoading, setChapterLoading] = useState(false);
  // 章节测验「导出答案」的独立 loading（用 leaf_id；与作业 courseware_id 分开，避免撞号）
  const [exportingQuiz, setExportingQuiz] = useState<number | null>(null);
  // 章节任务请求序号：消除快速切课时旧结果写回新课
  const chapterReqRef = useRef(0);
  const [typeFilter, setTypeFilter] = useState<number[]>([]);
  const [selectedTaskIds, setSelectedTaskIds] = useState<number[]>([]);
  const [nameFilter, setNameFilter] = useState('');

  // ===== 自动答题状态 =====
  const [answering, setAnswering] = useState(false); // 答题进行中（与导出互斥）
  const [quizDrawerOpen, setQuizDrawerOpen] = useState(false); // 进度抽屉开关
  const [quizDryRun, setQuizDryRun] = useState(false); // 当前是否「试跑」（仅展示用）
  const [quizName, setQuizName] = useState(''); // 当前答题的测验名
  const [quizEvents, setQuizEvents] = useState<QuizAnswerEvent[]>([]); // 逐题进度
  const [quizResult, setQuizResult] = useState<QuizAnswerResult | null>(null); // 汇总
  // 当前答题测验的 leaf_id（供「停止」使用）
  const currentQuizLeafRef = useRef<number | null>(null);

  // ===== 完成情况 / 得分 =====
  // schedule: { leaf_id: 完成度 }（1=完成、0=未完成、测验为浮点）
  const [schedule, setSchedule] = useState<Record<string, number>>({});
  const [totalSchedule, setTotalSchedule] = useState(0); // 整体完成度 0~1
  // quizScores: { leaf_id: 得分汇总 }（来自本地已导出 quiz_json）
  const [quizScores, setQuizScores] = useState<Record<string, QuizScore>>({});
  const scheduleReqRef = useRef(0); // 防快速切课竞态

  // 拉取章节任务（含测验/练习）；用请求序号防止快速切课的旧结果覆盖新课
  const fetchChapterTasks = useCallback(async () => {
    if (!id) return;
    const seq = ++chapterReqRef.current;
    setChapterLoading(true);
    try {
      const tasks = await invoke<ChapterTask[]>('get_chapter_tasks', { courseId: id });
      if (seq !== chapterReqRef.current) return; // 已切课，丢弃旧结果
      setChapterTasks(tasks);
    } catch (e) {
      if (seq === chapterReqRef.current) {
        message.error(`获取章节任务失败: ${normalizeError(e).message}`);
      }
    } finally {
      if (seq === chapterReqRef.current) setChapterLoading(false);
    }
  }, [id]);

  // 拉取学习进度（完成情况，按 leaf_id）；用请求序号防快速切课竞态
  const fetchSchedule = useCallback(async () => {
    if (!id) return;
    const seq = ++scheduleReqRef.current;
    try {
      const res = await invoke<LearnSchedule>('get_learn_schedule', { courseId: id });
      if (seq !== scheduleReqRef.current) return;
      setSchedule(res.leaf_schedules || {});
      setTotalSchedule(typeof res.total_schedule === 'number' ? res.total_schedule : 0);
    } catch (e) {
      // 进度获取失败不打断主流程，仅提示
      if (seq === scheduleReqRef.current) {
        message.error(`获取完成情况失败: ${normalizeError(e).message}`);
      }
    }
  }, [id]);

  // 汇总本地已导出测验的得分（扫 answer 目录，幂等覆盖，无需防竞态）
  const fetchQuizScores = useCallback(async () => {
    try {
      const res = await invoke<Record<string, QuizScore>>('get_quiz_scores');
      setQuizScores(res || {});
    } catch {
      // 本地得分汇总失败静默（不影响主流程）
    }
  }, []);

  useEffect(() => {
    if (id) {
      // 切换到新课程时先清空旧 works/ppts，避免上一个课程的数据闪现
      setCourseContext(id);
      setChapterTasks([]);
      setSelectedTaskIds([]);
      setNameFilter('');
      setTypeFilter([]);
      setSchedule({});
      setTotalSchedule(0);
      fetchWorks(id);
      fetchPpts(id);
      // 「作业列表」Tab 一进入即加载章节测验数据（测验表与批量导出都依赖它）
      fetchChapterTasks();
      // 完成情况（schedule，按 leaf_id）+ 本地已导出测验得分
      fetchSchedule();
      fetchQuizScores();
    }
    // 切换课程 / 卸载时中断正在进行的批量导出，避免旧循环继续写 state 或发请求
    return () => {
      batchStopRef.current = true;
    };
  }, [id, fetchWorks, fetchPpts, setCourseContext, fetchChapterTasks, fetchSchedule, fetchQuizScores]);

  // 监听自动答题进度事件（独立事件名，不与刷课 study-* 混用）
  useTauriListens([
    {
      event: 'quiz-answer-progress',
      handler: (e) => {
        const evt = e.payload as QuizAnswerEvent;
        setQuizEvents((prev) => {
          // 同一题（index 相同）由 running 更新为 done/failed/skipped，否则追加
          const i = prev.findIndex((x) => x.index === evt.index);
          if (i >= 0) {
            const next = prev.slice();
            next[i] = evt;
            return next;
          }
          return [...prev, evt];
        });
      },
    },
    {
      event: 'quiz-answer-stopped',
      handler: () => {
        message.warning('已停止自动答题');
      },
    },
  ]);

  // 筛选后的任务列表
  const filteredTasks = chapterTasks.filter((t) => {
    if (typeFilter.length > 0 && !typeFilter.includes(t.leaf_type)) return false;
    if (nameFilter && !t.name.toLowerCase().includes(nameFilter.toLowerCase())) return false;
    return true;
  });

  // 章节测验/练习（leaf_type=6）——可单独导出答案，并纳入批量导出
  const quizzes = chapterTasks.filter((t) => t.leaf_type === 6);
  // 批量导出总数（作业 + 测验）
  const batchTotal = works.length + quizzes.length;

  const handleExportAnswer = async (work: Work) => {
    if (!id) return;
    setExporting(work.courseware_id);
    try {
      const path = await invoke<string>('export_work_answers', {
        courseId: id,
        workId: work.exam_id,
        workName: work.title,
        workType: work.work_type,
      });
      message.success(`答案已导出: ${path}`);
    } catch (e) {
      message.error(`导出失败: ${normalizeError(e).message}`);
    } finally {
      setExporting(null);
    }
  };

  const handleExportData = async (work: Work) => {
    if (!id) return;
    setExporting(work.courseware_id);
    try {
      const result = await invoke<ExportResult>('export_exam_data', {
        courseId: id,
        workId: work.exam_id,
        workName: work.title,
        workType: work.work_type,
      });
      message.success(result.message);
    } catch (e) {
      message.error(`导出失败: ${normalizeError(e).message}`);
    } finally {
      setExporting(null);
    }
  };

  const handleExportQuiz = async (quiz: ChapterTask) => {
    if (!id) return;
    setExportingQuiz(quiz.id);
    try {
      const path = await invoke<string>('export_quiz_answers', {
        courseId: id,
        leafId: String(quiz.id),
        quizName: quiz.name,
      });
      message.success(`答案已导出: ${path}`);
      // 导出后刷新该测验的本地得分
      fetchQuizScores();
    } catch (e) {
      message.error(`导出失败: ${normalizeError(e).message}`);
    } finally {
      setExportingQuiz(null);
    }
  };

  // 执行自动答题（dryRun=true 仅生成答案、不提交）
  const runQuiz = async (quiz: ChapterTask, dryRun: boolean) => {
    if (!id || answering) return;
    currentQuizLeafRef.current = quiz.id;
    setQuizName(quiz.name);
    setQuizDryRun(dryRun);
    setQuizEvents([]);
    setQuizResult(null);
    setQuizDrawerOpen(true);
    setAnswering(true);
    try {
      const res = await invoke<QuizAnswerResult>('start_quiz_auto_answer', {
        courseId: id,
        leafId: String(quiz.id),
        dryRun,
      });
      setQuizResult(res);
      if (dryRun) {
        message.success(`试跑完成：可作答 ${res.from_local + res.from_ai}/${res.total} 题`);
      } else {
        message.success(
          `答题完成：提交 ${res.submitted}/${res.total}，正确 ${res.correct}，失败 ${res.failed}，跳过 ${res.skipped}`,
        );
        // 提交后完成情况可能变化，刷新进度
        fetchSchedule();
      }
    } catch (e) {
      message.error(`自动答题失败: ${normalizeError(e).message}`);
    } finally {
      setAnswering(false);
      currentQuizLeafRef.current = null;
    }
  };

  // 自动答题前的强警示确认（真实提交、写成绩、不可逆）
  const handleAutoAnswer = (quiz: ChapterTask) => {
    Modal.confirm({
      title: '确认对该测验自动答题？',
      width: 540,
      okText: '我已知晓，开始答题',
      okButtonProps: { danger: true },
      cancelText: '取消',
      content: (
        <Alert
          type="warning"
          showIcon
          message="此操作会真实提交答案并写入成绩"
          description={
            <div>
              测验「{quiz.name}」将被<strong>逐题自动提交</strong>（约每秒 1 题），提交后
              <strong>无法撤销</strong>。优先使用本地题库，未命中时调用 AI 兜底。
              <br />
              请确认本测验<strong>允许重新作答</strong>，并知晓答错也会被提交。建议先用「试跑」预览。
            </div>
          }
        />
      ),
      onOk: () => runQuiz(quiz, false),
    });
  };

  // 停止当前自动答题
  const handleStopQuiz = () => {
    const leaf = currentQuizLeafRef.current;
    if (leaf == null) return;
    invoke('stop_quiz_auto_answer', { leafId: String(leaf) }).catch(() => {});
  };

  // 一键导出当前作业 + 章节测验的全部答案：串行执行 + 温和限速 + 单个失败隔离并汇总
  const handleBatchExportAnswers = async () => {
    const total = works.length + quizzes.length;
    if (!id || total === 0) return;
    batchStopRef.current = false;
    setBatchExporting(true);
    setBatchProgress({ done: 0, total });

    const failures: { title: string; reason: string }[] = [];
    let ok = 0;
    let done = 0;

    // 第一段：作业 / 考试（走 examination 链路）
    for (let i = 0; i < works.length; i++) {
      if (batchStopRef.current) break;
      const work = works[i];
      try {
        await invoke<string>('export_work_answers', {
          courseId: id,
          workId: work.exam_id,
          workName: work.title,
          workType: work.work_type,
        });
        ok += 1;
      } catch (e) {
        failures.push({ title: work.title, reason: normalizeError(e).message });
      }
      done += 1;
      setBatchProgress({ done, total });
      // 温和限速：每项之间间隔约 400ms（整体最后一项无需等待）
      if (done < total && !batchStopRef.current) {
        await new Promise((resolve) => setTimeout(resolve, 400));
      }
    }

    // 第二段：章节测验 / 练习（走 MOOC exercise 链路）
    for (let i = 0; i < quizzes.length; i++) {
      if (batchStopRef.current) break;
      const quiz = quizzes[i];
      try {
        await invoke<string>('export_quiz_answers', {
          courseId: id,
          leafId: String(quiz.id),
          quizName: quiz.name,
        });
        ok += 1;
      } catch (e) {
        failures.push({ title: quiz.name, reason: normalizeError(e).message });
      }
      done += 1;
      setBatchProgress({ done, total });
      if (done < total && !batchStopRef.current) {
        await new Promise((resolve) => setTimeout(resolve, 400));
      }
    }

    const stopped = batchStopRef.current;
    setBatchExporting(false);
    // 批量导出后刷新本地测验得分
    fetchQuizScores();

    if (failures.length === 0) {
      message.success(`${stopped ? '已停止，' : ''}成功导出 ${ok} 个答案`);
    } else {
      message.warning(
        `${stopped ? '已停止。' : ''}完成：成功 ${ok} 个，失败 ${failures.length} 个`,
      );
      Modal.info({
        title: '部分答案导出失败',
        width: 520,
        content: (
          <div style={{ maxHeight: 320, overflow: 'auto' }}>
            {failures.map((f, idx) => (
              <div key={idx} style={{ marginBottom: 4 }}>
                <Typography.Text strong>{f.title}</Typography.Text>
                <Typography.Text type="secondary">：{f.reason}</Typography.Text>
              </div>
            ))}
          </div>
        ),
      });
    }
  };

  const handleStudySelected = () => {
    if (selectedTaskIds.length === 0) {
      message.warning('请先勾选要刷的任务');
      return;
    }
    navigate(`/study/${id}`, { state: { taskIds: selectedTaskIds } });
  };

  const handleStudyAll = () => {
    navigate(`/study/${id}`);
  };

  const handleSelectAllFiltered = () => {
    const allFilteredIds = filteredTasks.map((t) => t.id);
    setSelectedTaskIds(allFilteredIds);
  };

  const statusColor = (status: string) => {
    switch (status) {
      case '已批改': return 'green';
      case '已提交': return 'blue';
      case '未提交': return 'orange';
      case '缺考': return 'red';
      default: return 'default';
    }
  };

  const leafTypeColor = (t: number) => {
    switch (t) {
      case 0: return 'blue';
      case 3: return 'cyan';
      case 4: return 'purple';
      case 6: return 'orange';
      default: return 'default';
    }
  };

  // 去除浮点长尾（如 9.000001 → 9），最多两位小数
  const round2 = (n: number) => Math.round(n * 100) / 100;

  // 完成情况：schedule[leafId] → 已完成 / 未完成 / 百分比
  const renderProgress = (leafId: number) => {
    const v = schedule[String(leafId)];
    if (v === undefined || v === null) return <span style={{ color: '#bbb' }}>-</span>;
    if (v >= 1) return <Tag color="green">已完成</Tag>;
    if (v <= 0) return <Tag>未完成</Tag>;
    return <Tag color="blue">{Math.round(v * 100)}%</Tag>;
  };

  // 得分：本地已导出测验汇总 quizScores[leafId] → 得分/满分（未导出显示 -）
  const renderScore = (leafId: number) => {
    const s = quizScores[String(leafId)];
    if (!s || s.count === 0) return <span style={{ color: '#bbb' }}>-</span>;
    return (
      <span>
        {round2(s.score)}/{round2(s.total)}
      </span>
    );
  };

  const workColumns = [
    { title: '序号', key: 'index', width: 60, render: (_: unknown, __: unknown, i: number) => i + 1 },
    { title: '作业名称', dataIndex: 'title', key: 'title' },
    {
      title: '状态',
      dataIndex: 'status',
      key: 'status',
      width: 80,
      render: (s: string) => <Tag color={statusColor(s)}>{s}</Tag>,
    },
    { title: '分数', dataIndex: 'score', key: 'score', width: 70, render: (v: number | null) => v ?? '-' },
    { title: '题目数', dataIndex: 'problem_count', key: 'problem_count', width: 70, render: (v: number | null) => v ?? '-' },
    {
      title: '操作',
      key: 'action',
      width: 200,
      render: (_: unknown, record: Work) => (
        <Space>
          <Button
            size="small"
            icon={<DownloadOutlined />}
            loading={exporting === record.courseware_id}
            disabled={batchExporting || answering}
            onClick={() => handleExportAnswer(record)}
          >
            导出答案
          </Button>
          <Button
            size="small"
            icon={<ExportOutlined />}
            loading={exporting === record.courseware_id}
            disabled={batchExporting || answering}
            onClick={() => handleExportData(record)}
          >
            导出数据
          </Button>
        </Space>
      ),
    },
  ];

  const quizColumns = [
    { title: '序号', key: 'index', width: 60, render: (_: unknown, __: unknown, i: number) => i + 1 },
    { title: '测验名称', dataIndex: 'name', key: 'name' },
    {
      title: '完成情况',
      key: 'progress',
      width: 90,
      render: (_: unknown, record: ChapterTask) => renderProgress(record.id),
    },
    {
      title: '得分',
      key: 'score',
      width: 90,
      render: (_: unknown, record: ChapterTask) => renderScore(record.id),
    },
    {
      title: '操作',
      key: 'action',
      width: 320,
      render: (_: unknown, record: ChapterTask) => (
        <Space>
          <Button
            size="small"
            icon={<DownloadOutlined />}
            loading={exportingQuiz === record.id}
            disabled={batchExporting || answering}
            onClick={() => handleExportQuiz(record)}
          >
            导出答案
          </Button>
          <Button
            size="small"
            icon={<ExperimentOutlined />}
            disabled={batchExporting || answering}
            onClick={() => runQuiz(record, true)}
          >
            试跑
          </Button>
          <Button
            size="small"
            danger
            icon={<RobotOutlined />}
            disabled={batchExporting || answering}
            onClick={() => handleAutoAnswer(record)}
          >
            自动答题
          </Button>
        </Space>
      ),
    },
  ];

  const pptColumns = [
    { title: '序号', key: 'index', width: 60, render: (_: unknown, __: unknown, i: number) => i + 1 },
    { title: '课件名称', dataIndex: 'title', key: 'title' },
    { title: '页数', dataIndex: 'count', key: 'count', width: 80 },
  ];

  const chapterColumns = [
    { title: '序号', dataIndex: 'index', key: 'index', width: 60 },
    { title: '任务名称', dataIndex: 'name', key: 'name' },
    {
      title: '类型',
      dataIndex: 'type_str',
      key: 'type_str',
      width: 100,
      render: (v: string, record: ChapterTask) => (
        <Tag color={leafTypeColor(record.leaf_type)}>{v}</Tag>
      ),
    },
    {
      title: '完成情况',
      key: 'progress',
      width: 90,
      render: (_: unknown, record: ChapterTask) => renderProgress(record.id),
    },
    {
      title: '得分',
      key: 'score',
      width: 90,
      render: (_: unknown, record: ChapterTask) =>
        record.leaf_type === 6 ? renderScore(record.id) : <span style={{ color: '#bbb' }}>-</span>,
    },
    { title: '任务 ID', dataIndex: 'id', key: 'id', width: 110 },
  ];

  // 章节任务统计
  const taskStats = chapterTasks.reduce(
    (acc, t) => {
      acc[t.type_str] = (acc[t.type_str] || 0) + 1;
      return acc;
    },
    {} as Record<string, number>,
  );

  const rowSelection = {
    selectedRowKeys: selectedTaskIds,
    onChange: (keys: React.Key[]) => {
      setSelectedTaskIds(keys as number[]);
    },
  };

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 16, marginBottom: 16 }}>
        <Button icon={<ArrowLeftOutlined />} onClick={() => navigate('/')}>
          返回
        </Button>
        <Title level={4} style={{ margin: 0 }}>课程详情</Title>
        <div style={{ flex: 1 }} />
        <Button
          type="primary"
          icon={<PlayCircleOutlined />}
          onClick={handleStudyAll}
        >
          全部刷课
        </Button>
      </div>

      <Card>
        <Spin spinning={worksLoading || pptsLoading}>
          <Tabs
            onChange={(key) => {
              if (key === 'chapters' && chapterTasks.length === 0) {
                fetchChapterTasks();
              }
            }}
            items={[
              {
                key: 'works',
                label: `作业列表 (${works.length})`,
                children: (
                  <>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
                      <Button
                        type="primary"
                        icon={<DownloadOutlined />}
                        loading={batchExporting}
                        disabled={batchTotal === 0 || exporting !== null || exportingQuiz !== null || answering}
                        onClick={handleBatchExportAnswers}
                      >
                        {batchExporting && batchProgress
                          ? `导出中 ${batchProgress.done}/${batchProgress.total}`
                          : `一键导出全部答案 (${batchTotal})`}
                      </Button>
                      {batchExporting && (
                        <Button
                          danger
                          onClick={() => {
                            batchStopRef.current = true;
                          }}
                        >
                          停止
                        </Button>
                      )}
                    </div>
                    <Table columns={workColumns} dataSource={works} rowKey="courseware_id" pagination={false} size="middle" />
                    <div style={{ marginTop: 24 }}>
                      <Typography.Text strong style={{ display: 'block', marginBottom: 12 }}>
                        章节测验/练习 ({quizzes.length})
                      </Typography.Text>
                      <Table
                        columns={quizColumns}
                        dataSource={quizzes}
                        rowKey="id"
                        pagination={quizzes.length > 50 ? { pageSize: 50 } : false}
                        size="middle"
                        locale={{ emptyText: '该课程暂无章节测验/练习' }}
                      />
                    </div>
                  </>
                ),
              },
              {
                key: 'ppts',
                label: `课件列表 (${ppts.length})`,
                children: (
                  <Table columns={pptColumns} dataSource={ppts} rowKey="courseware_id" pagination={false} size="middle" />
                ),
              },
              {
                key: 'chapters',
                label: (
                  <span>
                    <UnorderedListOutlined /> 章节任务 {chapterTasks.length > 0 ? `(${chapterTasks.length})` : ''}
                  </span>
                ),
                children: (
                  <Spin spinning={chapterLoading}>
                    {/* 统计栏 */}
                    {chapterTasks.length > 0 && (
                      <div style={{ marginBottom: 12 }}>
                        <Space wrap>
                          {Object.entries(taskStats).map(([type, count]) => (
                            <Tag key={type}>{type}: {count}</Tag>
                          ))}
                          <Tag color="blue">共 {chapterTasks.length} 个任务</Tag>
                          {totalSchedule > 0 && (
                            <Tag color="geekblue">整体完成度 {Math.round(totalSchedule * 100)}%</Tag>
                          )}
                          {typeFilter.length > 0 && (
                            <Tag color="green">筛选后: {filteredTasks.length} 个</Tag>
                          )}
                          {selectedTaskIds.length > 0 && (
                            <Tag color="orange">已选: {selectedTaskIds.length} 个</Tag>
                          )}
                        </Space>
                      </div>
                    )}

                    {/* 筛选与操作栏 */}
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12, flexWrap: 'wrap' }}>
                      <FilterOutlined style={{ color: '#999' }} />
                      <Select
                        mode="multiple"
                        allowClear
                        placeholder="按类型筛选"
                        style={{ minWidth: 200 }}
                        value={typeFilter}
                        onChange={(v) => {
                          setTypeFilter(v);
                          setSelectedTaskIds([]);
                        }}
                        options={LEAF_TYPE_OPTIONS.map((o) => ({
                          value: o.value,
                          label: <Tag color={o.color}>{o.label}</Tag>,
                        }))}
                      />
                      <Input
                        placeholder="搜索任务名称..."
                        value={nameFilter}
                        onChange={(e) => setNameFilter(e.target.value)}
                        allowClear
                        style={{ width: 200 }}
                      />

                      <div style={{ flex: 1 }} />

                      <Space>
                        <Button
                          size="small"
                          icon={<CheckSquareOutlined />}
                          onClick={handleSelectAllFiltered}
                          disabled={filteredTasks.length === 0}
                        >
                          全选当前
                        </Button>
                        <Button
                          size="small"
                          onClick={() => setSelectedTaskIds([])}
                          disabled={selectedTaskIds.length === 0}
                        >
                          清空选择
                        </Button>
                        <Button
                          type="primary"
                          size="small"
                          icon={<PlayCircleOutlined />}
                          onClick={handleStudySelected}
                          disabled={selectedTaskIds.length === 0}
                        >
                          刷选中 ({selectedTaskIds.length})
                        </Button>
                        <Button
                          size="small"
                          icon={<ReloadOutlined />}
                          onClick={fetchChapterTasks}
                          loading={chapterLoading}
                        >
                          刷新
                        </Button>
                      </Space>
                    </div>

                    <Table
                      rowSelection={rowSelection}
                      columns={chapterColumns}
                      dataSource={filteredTasks}
                      rowKey="id"
                      pagination={filteredTasks.length > 50 ? { pageSize: 50 } : false}
                      size="middle"
                    />
                  </Spin>
                ),
              },
            ]}
          />
        </Spin>
      </Card>

      <Drawer
        title={quizDryRun ? '自动答题 · 试跑（不提交）' : '自动答题进度'}
        width={520}
        open={quizDrawerOpen}
        onClose={() => {
          if (!answering) setQuizDrawerOpen(false);
        }}
        maskClosable={!answering}
        extra={
          answering ? (
            <Button danger size="small" onClick={handleStopQuiz}>
              停止
            </Button>
          ) : null
        }
      >
        {quizName && (
          <Typography.Paragraph strong style={{ marginBottom: 8 }}>
            {quizName}
          </Typography.Paragraph>
        )}
        {quizDryRun && (
          <Alert
            type="info"
            showIcon
            style={{ marginBottom: 12 }}
            message="试跑模式：仅展示将提交的答案，不会真正提交。"
          />
        )}
        {(() => {
          const total =
            quizEvents.length > 0
              ? quizEvents[quizEvents.length - 1].total
              : quizResult?.total ?? 0;
          const done = quizEvents.filter((e) => e.status !== 'running').length;
          return total > 0 ? (
            <Progress
              percent={Math.round((done / total) * 100)}
              status={answering ? 'active' : 'normal'}
            />
          ) : null;
        })()}
        <List
          size="small"
          style={{ marginTop: 12 }}
          dataSource={quizEvents}
          locale={{ emptyText: answering ? '正在准备...' : '暂无进度' }}
          renderItem={(e: QuizAnswerEvent) => (
            <List.Item>
              <Space size="small" wrap>
                <span style={{ color: '#999' }}>#{e.index}</span>
                {e.source && (
                  <Tag color={e.source === 'local' ? 'green' : 'blue'}>
                    {e.source === 'local' ? '题库' : 'AI'}
                  </Tag>
                )}
                {e.status === 'done' && e.is_correct === true && (
                  <CheckCircleOutlined style={{ color: '#52c41a' }} />
                )}
                {e.status === 'done' && e.is_correct === false && (
                  <CloseCircleOutlined style={{ color: '#ff4d4f' }} />
                )}
                {e.status === 'failed' && <Tag color="red">失败</Tag>}
                {e.status === 'skipped' && <Tag>跳过</Tag>}
                {e.status === 'running' && <Tag color="processing">作答中</Tag>}
                <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                  {e.problem_id ? `题 ${e.problem_id} ` : ''}
                  {e.message || ''}
                </Typography.Text>
              </Space>
            </List.Item>
          )}
        />
        {quizResult && (
          <Alert
            style={{ marginTop: 12 }}
            type={quizResult.failed > 0 ? 'warning' : 'success'}
            showIcon
            message={quizDryRun ? '试跑汇总' : '答题汇总'}
            description={
              quizDryRun
                ? `共 ${quizResult.total} 题，可作答 ${quizResult.from_local + quizResult.from_ai}（题库 ${quizResult.from_local}、AI ${quizResult.from_ai}），跳过 ${quizResult.skipped}`
                : `共 ${quizResult.total} 题，提交 ${quizResult.submitted}（题库 ${quizResult.from_local}、AI ${quizResult.from_ai}），正确 ${quizResult.correct}，失败 ${quizResult.failed}，跳过 ${quizResult.skipped}`
            }
          />
        )}
      </Drawer>
    </div>
  );
}
