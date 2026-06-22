import { useEffect, useState } from 'react';
import { Table, Card, Typography, Button, Spin, Tag, message, Empty, Popconfirm, Space } from 'antd';
import {
  DeleteOutlined,
  ReloadOutlined,
  FileExcelOutlined,
  ExportOutlined,
} from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import { normalizeError } from '../utils/errors';
import PageHeader from '../components/PageHeader';

interface ExamFileInfo {
  exam_id: string;
  exam_name: string;
  has_question: boolean;
  has_answer: boolean;
  can_export: boolean;
}

interface DeleteBatchResult {
  deleted: number;
  failed: { exam_id: string; reason: string }[];
}

export default function ExamExport() {
  const [files, setFiles] = useState<ExamFileInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [exporting, setExporting] = useState<string | null>(null);
  const [selectedKeys, setSelectedKeys] = useState<string[]>([]);
  const [batchDeleting, setBatchDeleting] = useState(false);
  const [deletingOne, setDeletingOne] = useState<string | null>(null);

  const fetchFiles = async () => {
    setLoading(true);
    try {
      const data = await invoke<ExamFileInfo[]>('get_exam_files');
      setFiles(data);
      setSelectedKeys((prev) => {
        const existing = new Set(data.map((f) => f.exam_id));
        return prev.filter((k) => existing.has(k));
      });
    } catch (e) {
      const err = normalizeError(e);
      if (err.code === 'IO_ERROR') {
        setFiles([]);
        message.info('暂无考试数据，请先在课程详情中导出');
      } else {
        message.error(`获取考试文件失败: ${err.message}`);
      }
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
      message.error(`导出失败: ${normalizeError(e).message}`);
    } finally {
      setExporting(null);
    }
  };

  const handleDeleteOne = async (file: ExamFileInfo) => {
    setDeletingOne(file.exam_id);
    try {
      await invoke('delete_exam_file', { examId: file.exam_id });
      message.success(`已删除：${file.exam_name || file.exam_id}`);
      await fetchFiles();
    } catch (e) {
      message.error(`删除失败: ${normalizeError(e).message}`);
    } finally {
      setDeletingOne(null);
    }
  };

  const handleDeleteBatch = async () => {
    if (selectedKeys.length === 0) return;
    setBatchDeleting(true);
    try {
      const res = await invoke<DeleteBatchResult>('delete_exam_files', {
        examIds: selectedKeys,
      });
      if (res.failed.length === 0) {
        message.success(`已删除 ${selectedKeys.length} 组考试数据文件`);
      } else {
        message.warning(`删除完成：成功删除 ${res.deleted} 个文件，失败 ${res.failed.length} 组`);
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
      width: 230,
      render: (_: unknown, record: ExamFileInfo) => (
        <Space size={0}>
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
          <Popconfirm
            title="确定删除该组考试数据文件？"
            description="会删除本地题目文件和答案文件，不影响雨课堂平台数据。"
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
              loading={deletingOne === record.exam_id}
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
        icon={<ExportOutlined />}
        title="数据导出"
        extra={
          <Space>
            <Popconfirm
              title={`确定删除选中的 ${selectedKeys.length} 组考试数据文件？`}
              description="会删除本地题目文件和答案文件，不影响雨课堂平台数据。"
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
            <Empty description="暂无考试数据文件。请先在课程详情中导出考试数据。" />
          ) : (
            <>
              <Typography.Paragraph type="secondary" style={{ marginBottom: 16 }}>
                只有同时拥有题目文件和答案文件的考试才能导出为 Excel。
              </Typography.Paragraph>
              <Table
                rowSelection={{
                  selectedRowKeys: selectedKeys,
                  onChange: (keys) => setSelectedKeys(keys as string[]),
                }}
                columns={columns}
                dataSource={files}
                rowKey="exam_id"
                pagination={false}
                size="middle"
                scroll={{ x: 'max-content' }}
              />
            </>
          )}
        </Spin>
      </Card>
    </div>
  );
}
