import { useEffect, useState, useCallback } from 'react';
import { useParams, useNavigate, useLocation } from 'react-router-dom';
import { Card, Typography, Button, Tag, Progress, List, message, Popconfirm, Alert, Switch, Modal, Select, InputNumber, Space, theme } from 'antd';
import {
  ArrowLeftOutlined,
  PlayCircleOutlined,
  CheckCircleOutlined,
  StopOutlined,
} from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import type { Event } from '@tauri-apps/api/event';
import type { TaskUpdateEvent } from '../types';
import { useTauriListens } from '../utils/useTauriListens';
import { normalizeError } from '../utils/errors';
import PageHeader from '../components/PageHeader';

const { Text } = Typography;

interface Task {
  index: number;
  name: string;
  type_str: string;
  status: 'pending' | 'running' | 'done' | 'failed' | 'skipped';
  progress?: number;
  message?: string;
}

interface LocationState {
  taskIds?: number[];
}

export default function StudyProgress() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const location = useLocation();
  const locationState = (location.state || {}) as LocationState;
  const customTaskIds = locationState.taskIds;
  const { token } = theme.useToken();

  const [tasks, setTasks] = useState<Task[]>([]);
  const [running, setRunning] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [currentIndex, setCurrentIndex] = useState(0);
  const [totalTasks, setTotalTasks] = useState(0);
  const [startTime, setStartTime] = useState<number | null>(null);
  const [elapsed, setElapsed] = useState(0);
  // 刷课时是否对测验/练习真实自动答题（持久化、默认关）
  const [autoAnswerQuiz, setAutoAnswerQuiz] = useState<boolean>(() => {
    try {
      return localStorage.getItem('rc_auto_answer_quiz') === '1';
    } catch {
      return false;
    }
  });

  // 视频刷课播放倍率（持久化、默认 1x）
  const [playbackRate, setPlaybackRate] = useState<number>(() => {
    try {
      const v = parseFloat(localStorage.getItem('rc_playback_rate') || '1');
      return Number.isFinite(v) && v >= 1 && v <= 4 ? v : 1;
    } catch {
      return 1;
    }
  });

  const changePlaybackRate = (v: number) => {
    setPlaybackRate(v);
    try {
      localStorage.setItem('rc_playback_rate', String(v));
    } catch {
      /* localStorage 不可用时忽略持久化 */
    }
  };

  // 并行观看任务数（持久化、默认 1=串行）
  const [concurrency, setConcurrency] = useState<number>(() => {
    try {
      const v = parseInt(localStorage.getItem('rc_concurrency') || '1', 10);
      return Number.isFinite(v) && v >= 1 && v <= 10 ? v : 1;
    } catch {
      return 1;
    }
  });

  const changeConcurrency = (v: number | null) => {
    const n = Math.min(10, Math.max(1, Math.round(v ?? 1)));
    setConcurrency(n);
    try {
      localStorage.setItem('rc_concurrency', String(n));
    } catch {
      /* localStorage 不可用时忽略持久化 */
    }
  };

  // 切换开关：开启需强警示二次确认（真实提交、不可逆）；关闭直接生效
  const toggleAutoAnswer = (checked: boolean) => {
    if (!checked) {
      setAutoAnswerQuiz(false);
      try {
        localStorage.setItem('rc_auto_answer_quiz', '0');
      } catch {
        /* localStorage 不可用时忽略持久化 */
      }
      return;
    }
    Modal.confirm({
      title: '开启「刷课时自动答测验/练习」？',
      width: 540,
      okText: '我已知晓，开启',
      okButtonProps: { danger: true },
      cancelText: '取消',
      content: (
        <Alert
          type="warning"
          showIcon
          message="开启后刷课遇到测验/练习会真实提交答案并写入成绩"
          description={
            <div>
              刷课遇到<strong>测验/练习</strong>时将<strong>逐题自动提交</strong>（约每秒 1 题），提交后
              <strong>无法撤销</strong>。已提交过的小题会自动跳过；优先使用本地题库，未命中时调用 AI 兜底。
              关闭时测验仍按现状跳过。
            </div>
          }
        />
      ),
      onOk: () => {
        setAutoAnswerQuiz(true);
        try {
          localStorage.setItem('rc_auto_answer_quiz', '1');
        } catch {
          /* localStorage 不可用时忽略持久化 */
        }
      },
    });
  };

  // 计时器
  useEffect(() => {
    if (!running || !startTime) return;
    const timer = setInterval(() => setElapsed(Math.floor((Date.now() - startTime) / 1000)), 1000);
    return () => clearInterval(timer);
  }, [running, startTime]);

  // 安全订阅 Tauri 事件，修复 StrictMode 下 listen cleanup 竞态导致的双重监听
  useTauriListens([
    {
      event: 'study-task-update',
      handler: (event: Event<unknown>) => {
        const p = event.payload as TaskUpdateEvent;
        setCurrentIndex(p.index);
        if (p.total > 0) setTotalTasks(p.total);
        setTasks((prev) => {
          const copy = [...prev];
          const existing = copy.findIndex((t) => t.index === p.index);
          const task: Task = {
            index: p.index,
            name: p.name,
            type_str: p.type_str,
            status: p.status as Task['status'],
            progress: p.progress,
            message: p.message,
          };
          if (existing >= 0) {
            copy[existing] = task;
          } else {
            copy.push(task);
          }
          return copy;
        });
      },
    },
    {
      event: 'study-complete',
      handler: () => {
        setRunning(false);
        setStopping(false);
        message.success('所有任务处理完毕');
      },
    },
    {
      event: 'study-stopped',
      handler: () => {
        setRunning(false);
        setStopping(false);
        message.warning('刷课已停止');
      },
    },
  ]);

  const startStudy = useCallback(async () => {
    if (!id) return;
    if (running || stopping) return; // 防止重复点击
    setRunning(true);
    setStopping(false);
    setTasks([]);
    setTotalTasks(0);
    setStartTime(Date.now());
    setElapsed(0);
    try {
      await invoke('start_auto_study', {
        courseId: id,
        taskIds: customTaskIds ?? null,
        autoAnswerQuiz,
        playbackRate,
        concurrency,
      });
    } catch (e) {
      const err = normalizeError(e);
      if (err.code !== 'CANCELLED' && !stopping) {
        message.error(`启动失败: ${err.message}`);
      }
      setRunning(false);
      setStopping(false);
    }
  }, [id, running, stopping, customTaskIds, autoAnswerQuiz, playbackRate, concurrency]);

  const stopStudy = useCallback(async () => {
    setStopping(true);
    try {
      await invoke('stop_auto_study', { courseId: id });
      message.info('正在停止刷课...');
    } catch (e) {
      message.error(`停止失败: ${normalizeError(e).message}`);
      setStopping(false);
    }
  }, [id]);

  const statusTag = (status: string) => {
    const map: Record<string, { color: string; text: string }> = {
      pending: { color: 'default', text: '待处理' },
      running: { color: 'processing', text: '处理中' },
      done: { color: 'success', text: '已完成' },
      failed: { color: 'error', text: '失败' },
      skipped: { color: 'warning', text: '已跳过' },
    };
    const info = map[status] || map.pending;
    return <Tag color={info.color}>{info.text}</Tag>;
  };

  const doneCount = tasks.filter((t) => t.status === 'done' || t.status === 'skipped').length;
  const failedCount = tasks.filter((t) => t.status === 'failed').length;
  const effectiveTotal = totalTasks > 0 ? totalTasks : tasks.length;
  const overallPercent = effectiveTotal > 0 ? Math.round((doneCount / effectiveTotal) * 100) : 0;
  // 并行时可能有多个任务同时「处理中」
  const runningTasks = tasks.filter((t) => t.status === 'running');
  const currentTask = tasks.find((t) => t.index === currentIndex && t.status === 'running');
  // 并行到达顺序不定，按 index 稳定排序展示
  const sortedTasks = [...tasks].sort((a, b) => a.index - b.index);

  const formatTime = (secs: number) => {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return m > 0 ? `${m}分${s}秒` : `${s}秒`;
  };

  return (
    <div>
      <PageHeader
        before={
          <Button icon={<ArrowLeftOutlined />} onClick={() => navigate(-1)} disabled={running}>
            返回
          </Button>
        }
        title="自动刷课"
        extra={
          <>
            <Space size={6}>
              <Text type="secondary" style={{ fontSize: 13 }}>视频倍率</Text>
              <Select
                value={playbackRate}
                onChange={changePlaybackRate}
                disabled={running || stopping}
                style={{ width: 88 }}
                options={[
                  { value: 1, label: '1x' },
                  { value: 1.25, label: '1.25x' },
                  { value: 1.5, label: '1.5x' },
                  { value: 2, label: '2x' },
                  { value: 3, label: '3x' },
                ]}
              />
            </Space>
            <Space size={6}>
              <Text type="secondary" style={{ fontSize: 13 }}>并行</Text>
              <InputNumber
                value={concurrency}
                onChange={changeConcurrency}
                disabled={running || stopping}
                min={1}
                max={10}
                step={1}
                style={{ width: 72 }}
              />
            </Space>
            <Switch
              checked={autoAnswerQuiz}
              onChange={toggleAutoAnswer}
              disabled={running || stopping}
              checkedChildren="测验自动答题"
              unCheckedChildren="测验跳过"
            />
            {running ? (
              <Popconfirm
                title="确定停止刷课？"
                description="当前正在执行的任务会在完成当前步骤后停止"
                onConfirm={stopStudy}
                okText="确定停止"
                cancelText="继续"
                okButtonProps={{ danger: true }}
              >
                <Button
                  danger
                  icon={<StopOutlined />}
                  loading={stopping}
                  disabled={stopping}
                >
                  {stopping ? '正在停止...' : '停止刷课'}
                </Button>
              </Popconfirm>
            ) : (
              <Button
                type="primary"
                icon={<PlayCircleOutlined />}
                onClick={startStudy}
                disabled={running || stopping}
              >
                开始刷课
              </Button>
            )}
          </>
        }
      />

      {autoAnswerQuiz && (
        <Alert
          type="warning"
          showIcon
          style={{ marginBottom: 16 }}
          message="已开启「刷课时自动答测验/练习」：遇到测验会真实提交答案并写入成绩（不可逆）"
          description="已提交过的小题自动跳过；优先题库、未命中调用 AI 兜底。如需仅刷视频/课件，请关闭上方开关。"
        />
      )}

      {playbackRate >= 3 && (
        <Alert
          type="warning"
          showIcon
          style={{ marginBottom: 16 }}
          message={`已选 ${playbackRate}x 倍率`}
          description="真实倍速模式下，倍率越高、后台记录的观看时长越短（≈视频时长÷倍率）。真实播放器通常最高 2x，3x 及以上可信度略低，建议常用 1.5x~2x。"
        />
      )}

      {concurrency >= 4 && (
        <Alert
          type="warning"
          showIcon
          style={{ marginBottom: 16 }}
          message={`已设并行 ${concurrency}（同时观看 ${concurrency} 个任务）`}
          description="并行越多总耗时越短，但同一账号会同时发出更多心跳，被风控判定异常的风险也越高。建议 2~3 个。"
        />
      )}

      {customTaskIds && (
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 16 }}
          message={`自定义刷课模式：已选择 ${customTaskIds.length} 个任务`}
          description="只会执行选中的任务，其余任务将被跳过"
        />
      )}

      {(tasks.length > 0 || totalTasks > 0) && (
        <Card style={{ marginBottom: 16 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 16, marginBottom: 8 }}>
            <CheckCircleOutlined style={{ fontSize: 24, color: overallPercent >= 100 ? token.colorSuccess : token.colorPrimary }} />
            <div style={{ flex: 1 }}>
              <Progress
                percent={overallPercent}
                status={stopping ? 'exception' : running ? 'active' : overallPercent >= 100 ? 'success' : 'normal'}
                format={(p) => `${p}%`}
              />
            </div>
          </div>
          <div style={{ display: 'flex', gap: 24, flexWrap: 'wrap', fontSize: 13, color: token.colorTextSecondary }}>
            <span>总进度: <Text strong>{doneCount}</Text> / {effectiveTotal}</span>
            {failedCount > 0 && <span style={{ color: token.colorError }}>失败: {failedCount}</span>}
            {elapsed > 0 && <span>已用时: {formatTime(elapsed)}</span>}
            {running && doneCount > 0 && effectiveTotal > doneCount && (
              <span>
                预计剩余: {formatTime(Math.round(((elapsed / doneCount) * (effectiveTotal - doneCount))))}
              </span>
            )}
            {runningTasks.length > 1 ? (
              <span style={{ color: token.colorPrimary }}>
                进行中: {runningTasks.length} 个
              </span>
            ) : (
              currentTask && (
                <span style={{ color: token.colorPrimary }}>
                  当前: {currentTask.name}
                </span>
              )
            )}
          </div>
        </Card>
      )}

      <Card>
        <List
          dataSource={sortedTasks}
          locale={{ emptyText: '点击"开始刷课"启动自动学习任务' }}
          renderItem={(task) => (
            <List.Item
              actions={[statusTag(task.status)]}
              style={{
                opacity: task.status === 'done' || task.status === 'skipped' ? 0.6 : 1,
                background: task.index === currentIndex && task.status === 'running' ? token.colorPrimaryBg : undefined,
                padding: '8px 16px',
                borderRadius: 4,
              }}
            >
              <List.Item.Meta
                title={
                  <span>
                    <Text type="secondary">#{task.index} </Text>
                    {task.name}
                    <Tag style={{ marginLeft: 8 }}>{task.type_str}</Tag>
                  </span>
                }
                description={task.message || undefined}
              />
              {task.progress !== undefined && task.progress > 0 && (
                <Progress
                  type="circle"
                  size={36}
                  percent={Math.round(task.progress * 100)}
                />
              )}
            </List.Item>
          )}
        />
      </Card>
    </div>
  );
}
