import { useEffect, useState } from 'react';
import { useParams, useNavigate, useLocation } from 'react-router-dom';
import { Card, Typography, Button, Tag, Progress, List, Spin, message, Popconfirm, Alert } from 'antd';
import {
  ArrowLeftOutlined,
  PlayCircleOutlined,
  CheckCircleOutlined,
  StopOutlined,
} from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { TaskUpdateEvent } from '../types';

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

  useEffect(() => {
    const unlistenUpdate = listen<TaskUpdateEvent>('study-task-update', (event) => {
      const p = event.payload;
      setCurrentIndex(p.index);
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
    });

    const unlistenComplete = listen('study-complete', () => {
      setRunning(false);
      setStopping(false);
      message.success('所有任务处理完毕');
    });

    const unlistenStopped = listen('study-stopped', () => {
      setRunning(false);
      setStopping(false);
      message.warning('刷课已停止');
    });

    return () => {
      unlistenUpdate.then((fn) => fn());
      unlistenComplete.then((fn) => fn());
      unlistenStopped.then((fn) => fn());
    };
  }, []);

  const startStudy = async () => {
    if (!id) return;
    setRunning(true);
    setStopping(false);
    setTasks([]);
    try {
      await invoke('start_auto_study', {
        courseId: id,
        taskIds: customTaskIds ?? null,
      });
    } catch (e) {
      if (!stopping) {
        message.error(`启动失败: ${e}`);
      }
      setRunning(false);
      setStopping(false);
    }
  };

  const stopStudy = async () => {
    setStopping(true);
    try {
      await invoke('stop_auto_study');
      message.info('正在停止刷课...');
    } catch (e) {
      message.error(`停止失败: ${e}`);
      setStopping(false);
    }
  };

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
              icon={stopping ? <Spin size="small" /> : <StopOutlined />}
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

      {tasks.length > 0 && (
        <Card style={{ marginBottom: 16 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
            <CheckCircleOutlined style={{ fontSize: 24, color: '#52c41a' }} />
            <div style={{ flex: 1 }}>
              <Progress
                percent={tasks.length > 0 ? Math.round((doneCount / tasks.length) * 100) : 0}
                status={stopping ? 'exception' : running ? 'active' : 'normal'}
              />
            </div>
            <Text type="secondary">
              {doneCount} / {tasks.length}
            </Text>
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
