import { useEffect, useState } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { Table, Tabs, Typography, Button, Space, Card, Spin, Tag, message } from 'antd';
import {
  DownloadOutlined,
  PlayCircleOutlined,
  ExportOutlined,
  ArrowLeftOutlined,
  UnorderedListOutlined,
  ReloadOutlined,
} from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import { useCourseStore } from '../stores/courseStore';
import type { Work, Ppt, ExportResult } from '../types';

const { Title } = Typography;

interface ChapterTask {
  index: number;
  id: number;
  name: string;
  leaf_type: number;
  type_str: string;
}

export default function CourseDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { works, ppts, loading, fetchWorks, fetchPpts } = useCourseStore();
  const [exporting, setExporting] = useState<number | null>(null);
  const [chapterTasks, setChapterTasks] = useState<ChapterTask[]>([]);
  const [chapterLoading, setChapterLoading] = useState(false);

  useEffect(() => {
    if (id) {
      fetchWorks(id);
      fetchPpts(id);
    }
  }, [id, fetchWorks, fetchPpts]);

  const fetchChapterTasks = async () => {
    if (!id) return;
    setChapterLoading(true);
    try {
      const tasks = await invoke<ChapterTask[]>('get_chapter_tasks', { courseId: id });
      setChapterTasks(tasks);
    } catch (e) {
      message.error(`获取章节任务失败: ${e}`);
    } finally {
      setChapterLoading(false);
    }
  };

  const handleExportAnswer = async (work: Work) => {
    if (!id) return;
    setExporting(work.courseware_id);
    try {
      const path = await invoke<string>('export_work_answers', {
        courseId: id,
        workId: String(work.courseware_id),
        workName: work.title,
      });
      message.success(`答案已导出: ${path}`);
    } catch (e) {
      message.error(`导出失败: ${e}`);
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
        workId: String(work.courseware_id),
        workName: work.title,
        workType: work.work_type,
      });
      message.success(result.message);
    } catch (e) {
      message.error(`导出失败: ${e}`);
    } finally {
      setExporting(null);
    }
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
            onClick={() => handleExportAnswer(record)}
          >
            导出答案
          </Button>
          <Button
            size="small"
            icon={<ExportOutlined />}
            loading={exporting === record.courseware_id}
            onClick={() => handleExportData(record)}
          >
            导出数据
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
          onClick={() => navigate(`/study/${id}`)}
        >
          开始刷课
        </Button>
      </div>

      <Card>
        <Spin spinning={loading}>
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
                  <Table columns={workColumns} dataSource={works} rowKey="courseware_id" pagination={false} size="middle" />
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
                    {chapterTasks.length > 0 && (
                      <div style={{ marginBottom: 12 }}>
                        <Space>
                          {Object.entries(taskStats).map(([type, count]) => (
                            <Tag key={type}>{type}: {count}</Tag>
                          ))}
                          <Tag color="blue">共 {chapterTasks.length} 个任务</Tag>
                        </Space>
                      </div>
                    )}
                    <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 8 }}>
                      <Button
                        size="small"
                        icon={<ReloadOutlined />}
                        onClick={fetchChapterTasks}
                        loading={chapterLoading}
                      >
                        刷新
                      </Button>
                    </div>
                    <Table
                      columns={chapterColumns}
                      dataSource={chapterTasks}
                      rowKey="id"
                      pagination={chapterTasks.length > 50 ? { pageSize: 50 } : false}
                      size="middle"
                    />
                  </Spin>
                ),
              },
            ]}
          />
        </Spin>
      </Card>
    </div>
  );
}
