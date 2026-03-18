import { useEffect, useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, List, Button, Divider, Typography, Spin, message, Space, Empty } from 'antd';
import { QrcodeOutlined, UserOutlined, ReloadOutlined } from '@ant-design/icons';
import { QRCodeSVG } from 'qrcode.react';
import { listen } from '@tauri-apps/api/event';
import { useAuthStore } from '../stores/authStore';
import type { QrCodeEvent, LoginSuccessEvent } from '../types';

const { Title, Text } = Typography;

export default function Login() {
  const navigate = useNavigate();
  const { initClient, fetchSavedUsers, loginWithSession, startQrLogin, savedUsers, loading, isLoggedIn } = useAuthStore();
  const [qrUrl, setQrUrl] = useState<string | null>(null);
  const [qrLoading, setQrLoading] = useState(false);

  useEffect(() => {
    initClient().then(() => fetchSavedUsers());
  }, [initClient, fetchSavedUsers]);

  useEffect(() => {
    if (isLoggedIn) {
      navigate('/', { replace: true });
    }
  }, [isLoggedIn, navigate]);

  // 监听 QR 码事件
  useEffect(() => {
    const unlistenQr = listen<QrCodeEvent>('qr-code', (event) => {
      setQrUrl(event.payload.url);
      setQrLoading(false);
    });

    const unlistenLogin = listen<LoginSuccessEvent>('login-success', (event) => {
      message.success(`登录成功: ${event.payload.name}`);
    });

    return () => {
      unlistenQr.then((fn) => fn());
      unlistenLogin.then((fn) => fn());
    };
  }, []);

  const handleQrLogin = useCallback(async () => {
    setQrLoading(true);
    setQrUrl(null);
    try {
      await startQrLogin();
    } catch {
      setQrLoading(false);
    }
  }, [startQrLogin]);

  const handleSessionLogin = useCallback(
    async (username: string) => {
      try {
        await loginWithSession(username);
        message.success(`已切换到用户: ${username}`);
      } catch {
        message.error('会话已过期，请重新扫码登录');
      }
    },
    [loginWithSession],
  );

  return (
    <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', minHeight: '100vh', background: '#f5f5f5' }}>
      <Card style={{ width: 800, boxShadow: '0 4px 12px rgba(0,0,0,0.08)' }}>
        <Title level={3} style={{ textAlign: 'center', marginBottom: 32 }}>
          雨课堂助手 v2
        </Title>

        <div style={{ display: 'flex', gap: 32 }}>
          {/* 左侧：已保存用户 */}
          <div style={{ flex: 1 }}>
            <Title level={5}>
              <UserOutlined /> 已保存的用户
            </Title>
            {savedUsers.length === 0 ? (
              <Empty description="暂无保存的用户" />
            ) : (
              <List
                size="small"
                bordered
                dataSource={savedUsers}
                renderItem={(user) => (
                  <List.Item
                    actions={[
                      <Button
                        type="link"
                        size="small"
                        loading={loading}
                        onClick={() => handleSessionLogin(user)}
                      >
                        登录
                      </Button>,
                    ]}
                  >
                    <List.Item.Meta avatar={<UserOutlined />} title={user} />
                  </List.Item>
                )}
              />
            )}
            <Button
              icon={<ReloadOutlined />}
              style={{ marginTop: 8 }}
              onClick={fetchSavedUsers}
              block
            >
              刷新列表
            </Button>
          </div>

          <Divider type="vertical" style={{ height: 'auto' }} />

          {/* 右侧：扫码登录 */}
          <div style={{ flex: 1, textAlign: 'center' }}>
            <Title level={5}>
              <QrcodeOutlined /> 微信扫码登录
            </Title>

            <div
              style={{
                width: 220,
                height: 220,
                margin: '16px auto',
                display: 'flex',
                justifyContent: 'center',
                alignItems: 'center',
                border: '1px solid #d9d9d9',
                borderRadius: 8,
                background: '#fafafa',
              }}
            >
              {qrLoading && !qrUrl ? (
                <Spin tip="正在获取二维码..." />
              ) : qrUrl ? (
                <QRCodeSVG value={qrUrl} size={200} />
              ) : (
                <Text type="secondary">点击下方按钮获取二维码</Text>
              )}
            </div>

            <Space direction="vertical" style={{ width: '100%' }}>
              <Button
                type="primary"
                icon={<QrcodeOutlined />}
                onClick={handleQrLogin}
                loading={qrLoading || loading}
                block
              >
                {qrUrl ? '刷新二维码' : '获取二维码'}
              </Button>
              {qrUrl && (
                <Text type="secondary" style={{ fontSize: 12 }}>
                  请使用微信扫描上方二维码
                </Text>
              )}
            </Space>
          </div>
        </div>
      </Card>
    </div>
  );
}
