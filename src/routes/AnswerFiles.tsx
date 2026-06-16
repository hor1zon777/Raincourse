import { useEffect, useState } from 'react';
import { Table, Card, Button, Empty, Spin, Space, Popconfirm, message } from 'antd';
import {
  ReloadOutlined,
  FileTextOutlined,
  DeleteOutlined,
  UploadOutlined,
  DownloadOutlined,
} from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import { save, open } from '@tauri-apps/plugin-dialog';
import type { AnswerFile } from '../types';
import { normalizeError } from '../utils/errors';
import PageHeader from '../components/PageHeader';

interface DeleteBatchResult {
  deleted: number;
  failed: { file_name: string; reason: string }[];
}

interface ImportResult {
  imported: number;
  skipped: number;
  failed: { file_name: string; reason: string }[];
}

export default function AnswerFiles() {
  const [files, setFiles] = useState<AnswerFile[]>([]);
  const [loading, setLoading] = useState(false);
  // 选中的文件（以真实 file_name 为键）
  const [selectedKeys, setSelectedKeys] = useState<string[]>([]);
  // 批量删除进行中
  const [batchDeleting, setBatchDeleting] = useState(false);
  // 单个删除进行中的 file_name
  const [deletingOne, setDeletingOne] = useState<string | null>(null);
  // 导出分享包进行中
  const [exporting, setExporting] = useState(false);
  // 导入分享包进行中
  const [importing, setImporting] = useState(false);

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

  // 删除单个答案文件
  const handleDeleteOne = async (record: AnswerFile) => {
    setDeletingOne(record.file_name);
    try {
      await invoke('delete_answer_file', { fileName: record.file_name });
      message.success(`已删除：${record.exam_name || record.file_name}`);
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
      setSelectedKeys([]);
      await fetchFiles();
    } catch (e) {
      message.error(`批量删除失败: ${normalizeError(e).message}`);
    } finally {
      setBatchDeleting(false);
    }
  };

  // 把选中的答案文件打包导出为单个分享包
  const handleExportBundle = async () => {
    if (selectedKeys.length === 0) return;
    // 选保存路径（默认文件名带日期，便于区分）
    const today = new Date().toISOString().slice(0, 10);
    let dest: string | null = null;
    try {
      dest = await save({
        title: '导出答案分享包',
        defaultPath: `雨课堂答案_${today}.json`,
        filters: [{ name: '答案分享包', extensions: ['json'] }],
      });
    } catch (e) {
      message.error(`打开保存对话框失败: ${normalizeError(e).message}`);
      return;
    }
    if (!dest) return; // 用户取消

    setExporting(true);
    try {
      const count = await invoke<number>('export_answer_bundle', {
        fileNames: selectedKeys,
        destPath: dest,
      });
      message.success(`已导出 ${count} 个答案到分享包：${dest}`);
    } catch (e) {
      message.error(`导出失败: ${normalizeError(e).message}`);
    } finally {
      setExporting(false);
    }
  };

  // 从分享包导入答案文件（已存在同名文件跳过）
  const handleImportBundle = async () => {
    let src: string | null = null;
    try {
      const picked = await open({
        title: '导入答案分享包',
        multiple: false,
        directory: false,
        filters: [{ name: '答案分享包', extensions: ['json'] }],
      });
      // multiple:false 时返回 string | null
      src = typeof picked === 'string' ? picked : null;
    } catch (e) {
      message.error(`打开文件对话框失败: ${normalizeError(e).message}`);
      return;
    }
    if (!src) return; // 用户取消

    setImporting(true);
    try {
      const res = await invoke<ImportResult>('import_answer_bundle', {
        srcPath: src,
      });
      const parts = [`导入 ${res.imported} 个`];
      if (res.skipped > 0) parts.push(`跳过 ${res.skipped} 个（已存在）`);
      if (res.failed.length > 0) parts.push(`失败 ${res.failed.length} 个`);
      const summary = parts.join('，');
      if (res.failed.length > 0) {
        message.warning(summary);
      } else {
        message.success(summary);
      }
      await fetchFiles();
    } catch (e) {
      message.error(`导入失败: ${normalizeError(e).message}`);
    } finally {
      setImporting(false);
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
      width: 90,
      render: (_: unknown, record: AnswerFile) => (
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
            <Button
              icon={<UploadOutlined />}
              onClick={handleImportBundle}
              loading={importing}
            >
              导入
            </Button>
            <Button
              icon={<DownloadOutlined />}
              onClick={handleExportBundle}
              disabled={selectedKeys.length === 0}
              loading={exporting}
            >
              导出选中{selectedKeys.length > 0 ? ` (${selectedKeys.length})` : ''}
            </Button>
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
    </div>
  );
}
