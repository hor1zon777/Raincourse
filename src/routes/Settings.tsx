import { useEffect, useState } from 'react';
import { Card, Typography, Space, Form, Input, Switch, Button, message } from 'antd';
import { SettingOutlined, RobotOutlined } from '@ant-design/icons';
import { invoke } from '@tauri-apps/api/core';
import { normalizeError } from '../utils/errors';
import PageHeader from '../components/PageHeader';
import type { AiConfig, AiConfigView } from '../types';

export default function Settings() {
  const [form] = Form.useForm();
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  // 后端不回传明文 key，仅告知是否已配置，用于输入框占位提示
  const [hasApiKey, setHasApiKey] = useState(false);

  useEffect(() => {
    let mounted = true;
    (async () => {
      setLoading(true);
      try {
        const cfg = await invoke<AiConfigView>('get_ai_config');
        if (!mounted) return;
        form.setFieldsValue({
          base_url: cfg.base_url,
          model: cfg.model,
          enabled: cfg.enabled,
        });
        setHasApiKey(cfg.has_api_key);
      } catch (e) {
        message.error(`读取 AI 配置失败: ${normalizeError(e).message}`);
      } finally {
        if (mounted) setLoading(false);
      }
    })();
    return () => {
      mounted = false;
    };
  }, [form]);

  const handleSave = async () => {
    const values = await form.validateFields();
    setSaving(true);
    try {
      const config: AiConfig = {
        base_url: (values.base_url || '').trim(),
        model: (values.model || '').trim(),
        enabled: !!values.enabled,
        // 留空表示沿用后端已存的旧 key（后端处理）
        api_key: (values.api_key || '').trim(),
      };
      await invoke('save_ai_config', { config });
      message.success('AI 配置已保存');
      if (config.api_key) setHasApiKey(true);
      // 保存后清空输入框，避免明文 key 驻留在表单状态里
      form.setFieldValue('api_key', '');
    } catch (e) {
      message.error(`保存失败: ${normalizeError(e).message}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div>
      <PageHeader icon={<SettingOutlined />} title="设置" />

      <Space direction="vertical" style={{ width: '100%' }} size="large">
        <Card
          title={
            <span>
              <RobotOutlined /> AI 答题模型（OpenAI 兼容）
            </span>
          }
          loading={loading}
        >
          <Typography.Paragraph type="secondary" style={{ marginBottom: 16 }}>
            用于章节测验「自动答题」在本地题库未命中时兜底作答。支持任意 OpenAI 兼容接口
            （如 DeepSeek、智谱、OpenAI 等）。API Key 仅保存在本机并加密存储，不会上传。
          </Typography.Paragraph>
          <Form form={form} layout="vertical" requiredMark={false}>
            <Form.Item
              name="base_url"
              label="API 地址 (base_url)"
              rules={[{ required: true, message: '请填写 base_url' }]}
            >
              <Input placeholder="https://api.deepseek.com" allowClear />
            </Form.Item>
            <Form.Item
              name="model"
              label="模型名称"
              rules={[{ required: true, message: '请填写模型名称' }]}
            >
              <Input placeholder="deepseek-chat" allowClear />
            </Form.Item>
            <Form.Item name="api_key" label="API Key">
              <Input.Password
                placeholder={hasApiKey ? '已配置，留空则不修改' : 'sk-...'}
                autoComplete="off"
              />
            </Form.Item>
            <Form.Item name="enabled" label="启用 AI 兜底" valuePropName="checked">
              <Switch />
            </Form.Item>
            <Button type="primary" loading={saving} onClick={handleSave}>
              保存
            </Button>
          </Form>
        </Card>

        <Card title="数据存储位置">
          <Typography.Paragraph type="secondary">
            所有数据（会话、答案文件、导出数据、AI 配置）均存储在系统应用数据目录中。
            <br />
            Windows: <code>%APPDATA%/com.raincourse.app/</code>
          </Typography.Paragraph>
        </Card>
      </Space>
    </div>
  );
}
