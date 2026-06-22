import { useEffect, useRef, useState } from 'react';
import type { ChangeEvent } from 'react';
import { Alert, Button, Card, Empty, Popconfirm, Space, Spin, Table, message } from 'antd';
import {
  DeleteOutlined,
  ExportOutlined,
  EyeOutlined,
  FileTextOutlined,
  FolderOpenOutlined,
  ImportOutlined,
  ReloadOutlined,
} from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import { useNavigate } from 'react-router-dom';
import type { AnswerFile } from '../types';
import { normalizeError } from '../utils/errors';
import PageHeader from '../components/PageHeader';

interface DeleteBatchResult {
  deleted: number;
  failed: { file_name: string; reason: string }[];
}

interface AnswerExportPackage {
  type: 'raincourse-answer-export';
  version: number;
  exported_at: string;
  count: number;
  files: { file_name: string; data: unknown }[];
}

interface ImportAnswerItem {
  fileName: string;
  data: unknown;
}

interface ImportBatchResult {
  imported: number;
  failed: { file_name: string; reason: string }[];
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const downloadJson = (data: unknown, fileName: string) => {
  const blob = new Blob([JSON.stringify(data, null, 2)], {
    type: 'application/json;charset=utf-8',
  });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = fileName;
  document.body.appendChild(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
};

const exportFileName = () => {
  const stamp = new Date().toISOString().replace(/[:.]/g, '-');
  return `raincourse_answers_${stamp}.json`;
};

const parseImportJson = (fileName: string, data: unknown): ImportAnswerItem[] => {
  if (isRecord(data) && data.type === 'raincourse-answer-export' && Array.isArray(data.files)) {
    return data.files.map((item, index) => {
      if (!isRecord(item)) {
        throw new Error(`${fileName}: 第 ${index + 1} 个导入项格式错误`);
      }
      const itemFileName = item.file_name ?? item.fileName;
      if (typeof itemFileName !== 'string' || itemFileName.trim() === '') {
        throw new Error(`${fileName}: 第 ${index + 1} 个导入项缺少文件名`);
      }
      return {
        fileName: itemFileName,
        data: item.data,
      };
    });
  }
  return [{ fileName, data }];
};

export default function AnswerFiles() {
  const navigate = useNavigate();
  const importInputRef = useRef<HTMLInputElement | null>(null);
  const [files, setFiles] = useState<AnswerFile[]>([]);
  const [loading, setLoading] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  const [openingFolder, setOpeningFolder] = useState<'answer' | 'download' | null>(null);
  const [lastExportName, setLastExportName] = useState<string | null>(null);
  // 选中的文件（以真实 file_name 为键）
  const [selectedKeys, setSelectedKeys] = useState<string[]>([]);
  // 批量删除进行中
  const [batchDeleting, setBatchDeleting] = useState(false);
  // 单个删除进行中的 file_name
  const [deletingOne, setDeletingOne] = useState<string | null>(null);

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

  const handlePreview = (record: AnswerFile) => {
    navigate(`/answers/preview/${encodeURIComponent(record.file_name)}`);
  };

  const handleExport = async (fileNames: string[]) => {
    if (fileNames.length === 0) return;
    setExporting(true);
    try {
      const data = await invoke<AnswerExportPackage>('export_answer_files', {
        fileNames,
      });
      const fileName = exportFileName();
      downloadJson(data, fileName);
      setLastExportName(fileName);
      message.success(`已导出 ${data.count} 个答案文件：${fileName}`);
    } catch (e) {
      message.error(`导出失败: ${normalizeError(e).message}`);
    } finally {
      setExporting(false);
    }
  };

  const handleOpenFolder = async (type: 'answer' | 'download') => {
    setOpeningFolder(type);
    try {
      const command = type === 'answer' ? 'open_answer_folder' : 'open_download_folder';
      const path = await invoke<string>(command);
      message.success(`已打开文件夹: ${path}`);
    } catch (e) {
      message.error(`打开文件夹失败: ${normalizeError(e).message}`);
    } finally {
      setOpeningFolder(null);
    }
  };

  const handleImportClick = () => {
    importInputRef.current?.click();
  };

  const handleImportFiles = async (event: ChangeEvent<HTMLInputElement>) => {
    const input = event.currentTarget;
    const picked = Array.from(input.files ?? []);
    input.value = '';
    if (picked.length === 0) return;

    setImporting(true);
    try {
      const importItems: ImportAnswerItem[] = [];
      for (const file of picked) {
        const text = await file.text();
        let data: unknown;
        try {
          data = JSON.parse(text);
        } catch {
          throw new Error(`${file.name}: JSON 格式错误`);
        }
        importItems.push(...parseImportJson(file.name, data));
      }

      const res = await invoke<ImportBatchResult>('import_answer_files', {
        files: importItems,
      });
      if (res.failed.length === 0) {
        message.success(`已导入 ${res.imported} 个答案文件`);
      } else if (res.imported > 0) {
        message.warning(`导入完成：成功 ${res.imported} 个，失败 ${res.failed.length} 个`);
      } else {
        message.error(`导入失败：${res.failed[0]?.reason || '文件格式不正确'}`);
      }
      await fetchFiles();
    } catch (e) {
      message.error(`导入失败: ${normalizeError(e).message}`);
    } finally {
      setImporting(false);
    }
  };

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
      width: 160,
      render: (_: unknown, record: AnswerFile) => (
        <Space size={0}>
          <Button
            type="link"
            size="small"
            icon={<EyeOutlined />}
            onClick={() => handlePreview(record)}
          >
            预览
          </Button>
          <Button
            type="link"
            size="small"
            icon={<ExportOutlined />}
            loading={exporting}
            onClick={() => handleExport([record.file_name])}
          >
            导出
          </Button>
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
        </Space>
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
            <input
              ref={importInputRef}
              type="file"
              accept=".json,application/json"
              multiple
              style={{ display: 'none' }}
              onChange={handleImportFiles}
            />
            <Button icon={<ImportOutlined />} onClick={handleImportClick} loading={importing}>
              导入答案
            </Button>
            <Button
              icon={<ExportOutlined />}
              disabled={selectedKeys.length === 0}
              loading={exporting}
              onClick={() => handleExport(selectedKeys)}
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

      <Alert
        type={lastExportName ? 'success' : 'info'}
        showIcon
        style={{ marginBottom: 12 }}
        message={lastExportName ? `最近导出：${lastExportName}` : '答案文件位置'}
        description={
          <Space direction="vertical" size={8}>
            <span>
              本页展示应用内答案目录中的文件；“导出选中”会生成一个打包 JSON，并保存到系统下载目录。
            </span>
            <Space wrap>
              <Button
                size="small"
                icon={<FolderOpenOutlined />}
                loading={openingFolder === 'answer'}
                onClick={() => handleOpenFolder('answer')}
              >
                打开答案目录
              </Button>
              <Button
                size="small"
                icon={<FolderOpenOutlined />}
                loading={openingFolder === 'download'}
                onClick={() => handleOpenFolder('download')}
              >
                打开下载目录
              </Button>
            </Space>
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
