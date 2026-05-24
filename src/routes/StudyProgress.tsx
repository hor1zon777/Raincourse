import { useEffect, useState, useCallback } from 'react';
import { useParams, useNavigate, useLocation } from 'react-router-dom';
import { Card, Typography, Button, Tag, Progress, List, message, Popconfirm, Alert } from 'antd';
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

const { Title, Text } = Typography;

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

  const [tasks, setTasks] = useState<Task[]>([]);
  const [running, setRunning] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [currentIndex, setCurrentIndex] = useState(0);
  const [totalTasks, setTotalTasks] = useState(0);
  const [startTime, setStartTime] = useState<number | null>(null);
  const [elapsed, setElapsed] = useState(0);

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
      });
    } catch (e) {
      const err = normalizeError(e);
      if (err.code !== 'CANCELLED' && !stopping) {
        message.error(`启动失败: ${err.message}`);
      }
      setRunning(false);
      setStopping(false);
    }
  }, [id, running, stopping, customTaskIds]);

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
  const currentTask = tasks.find((t) => t.index === currentIndex && t.status === 'running');

  const formatTime = (secs: number) => {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return m > 0 ? `${m}分${s}秒` : `${s}秒`;
  };

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 16, marginBottom: 16 }}>
        <Button icon={<ArrowLeftOutlined />} onClick={() => navigate(-1)} disabled={running}>
          返回
        </Button>
        <Title level={4} style={{ margin: 0 }}>自动刷课</Title>
        <div style={{ flex: 1 }} />
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
      </div>

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
            <CheckCircleOutlined style={{ fontSize: 24, color: overallPercent >= 100 ? '#52c41a' : '#1677ff' }} />
            <div style={{ flex: 1 }}>
              <Progress
                percent={overallPercent}
                status={stopping ? 'exception' : running ? 'active' : overallPercent >= 100 ? 'success' : 'normal'}
                format={(p) => `${p}%`}
              />
            </div>
          </div>
          <div style={{ display: 'flex', gap: 24, flexWrap: 'wrap', fontSize: 13, color: '#666' }}>
            <span>总进度: <Text strong>{doneCount}</Text> / {effectiveTotal}</span>
            {failedCount > 0 && <span style={{ color: '#ff4d4f' }}>失败: {failedCount}</span>}
            {elapsed > 0 && <span>已用时: {formatTime(elapsed)}</span>}
            {running && doneCount > 0 && effectiveTotal > doneCount && (
              <span>
                预计剩余: {formatTime(Math.round(((elapsed / doneCount) * (effectiveTotal - doneCount))))}
              </span>
            )}
            {currentTask && (
              <span style={{ color: '#1677ff' }}>
                当前: {currentTask.name}
              </span>
            )}
          </div>
        </Card>
      )}

      <Card>
        <List
          dataSource={tasks}
          locale={{ emptyText: '点击"开始刷课"启动自动学习任务' }}
          renderItem={(task) => (
            <List.Item
              actions={[statusTag(task.status)]}
              style={{
                opacity: task.status === 'done' || task.status === 'skipped' ? 0.6 : 1,
                background: task.index === currentIndex && task.status === 'running' ? '#e6f7ff' : undefined,
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
