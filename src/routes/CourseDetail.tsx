import { useEffect, useState } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { Table, Tabs, Typography, Button, Space, Card, Spin, Tag, message, Modal } from 'antd';
import {
  DownloadOutlined,
  PlayCircleOutlined,
  ExportOutlined,
  ArrowLeftOutlined,
} from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import { useCourseStore } from '../stores/courseStore';
import type { Work, Ppt, ExportResult } from '../types';

const { Title } = Typography;

export default function CourseDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { works, ppts, loading, fetchWorks, fetchPpts } = useCourseStore();
  const [exporting, setExporting] = useState<number | null>(null);

  useEffect(() => {
    if (id) {
      fetchWorks(id);
      fetchPpts(id);
    }
  }, [id, fetchWorks, fetchPpts]);

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
            ]}
          />
        </Spin>
      </Card>
    </div>
  );
}
