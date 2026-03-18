import { useEffect, useState } from 'react';
import { Table, Card, Typography, Button, Empty, Spin } from 'antd';
import { ReloadOutlined, FileTextOutlined } from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import type { AnswerFile } from '../types';

const { Title } = Typography;

export default function AnswerFiles() {
  const [files, setFiles] = useState<AnswerFile[]>([]);
  const [loading, setLoading] = useState(false);

  const fetchFiles = async () => {
    setLoading(true);
    try {
      const data = await invoke<AnswerFile[]>('get_answer_files');
      setFiles(data);
    } catch {
      // 忽略
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchFiles();
  }, []);

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
  ];

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
        <Title level={4} style={{ margin: 0 }}>
          <FileTextOutlined /> 答案文件
        </Title>
        <Button icon={<ReloadOutlined />} onClick={fetchFiles} loading={loading}>
          刷新
        </Button>
      </div>

      <Card>
        <Spin spinning={loading}>
          {files.length === 0 ? (
            <Empty description="暂无答案文件" />
          ) : (
            <Table columns={columns} dataSource={files} rowKey="exam_id" pagination={false} size="middle" />
          )}
        </Spin>
      </Card>
    </div>
  );
}
