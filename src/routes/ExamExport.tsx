import { useEffect, useState } from 'react';
import { Table, Card, Typography, Button, Spin, Tag, message, Empty } from 'antd';
import { ReloadOutlined, FileExcelOutlined, ExportOutlined } from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';

const { Title } = Typography;

interface ExamFileInfo {
  exam_id: string;
  exam_name: string;
  has_question: boolean;
  has_answer: boolean;
  can_export: boolean;
}

export default function ExamExport() {
  const [files, setFiles] = useState<ExamFileInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [exporting, setExporting] = useState<string | null>(null);

  const fetchFiles = async () => {
    setLoading(true);
    try {
      const data = await invoke<ExamFileInfo[]>('get_exam_files');
      setFiles(data);
    } catch {
      // 目录可能还不存在
      setFiles([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchFiles();
  }, []);

  const handleExportExcel = async (file: ExamFileInfo) => {
    setExporting(file.exam_id);
    try {
      const path = await invoke<string>('export_excel', {
        examId: file.exam_id,
        examName: file.exam_name || file.exam_id,
      });
      message.success(`Excel 已导出: ${path}`);
    } catch (e) {
      message.error(`导出失败: ${e}`);
    } finally {
      setExporting(null);
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
      width: 130,
    },
    {
      title: '考试名称',
      dataIndex: 'exam_name',
      key: 'exam_name',
      render: (v: string) => v || '-',
    },
    {
      title: '题目文件',
      dataIndex: 'has_question',
      key: 'has_question',
      width: 90,
      render: (v: boolean) => (
        <Tag color={v ? 'green' : 'default'}>{v ? '有' : '无'}</Tag>
      ),
    },
    {
      title: '答案文件',
      dataIndex: 'has_answer',
      key: 'has_answer',
      width: 90,
      render: (v: boolean) => (
        <Tag color={v ? 'green' : 'default'}>{v ? '有' : '无'}</Tag>
      ),
    },
    {
      title: '操作',
      key: 'action',
      width: 140,
      render: (_: unknown, record: ExamFileInfo) => (
        <Button
          type="primary"
          size="small"
          icon={<FileExcelOutlined />}
          disabled={!record.can_export}
          loading={exporting === record.exam_id}
          onClick={() => handleExportExcel(record)}
        >
          导出 Excel
        </Button>
      ),
    },
  ];

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
        <Title level={4} style={{ margin: 0 }}>
          <ExportOutlined /> 数据导出
        </Title>
        <Button icon={<ReloadOutlined />} onClick={fetchFiles} loading={loading}>
          刷新
        </Button>
      </div>

      <Card>
        <Spin spinning={loading}>
          {files.length === 0 ? (
            <Empty description="暂无考试数据文件。请先在课程详情中导出考试数据。" />
          ) : (
            <>
              <Typography.Paragraph type="secondary" style={{ marginBottom: 16 }}>
                只有同时拥有题目文件和答案文件的考试才能导出为 Excel。
              </Typography.Paragraph>
              <Table
                columns={columns}
                dataSource={files}
                rowKey="exam_id"
                pagination={false}
                size="middle"
              />
            </>
          )}
        </Spin>
      </Card>
    </div>
  );
}
