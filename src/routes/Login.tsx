import { useEffect, useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, List, Button, Divider, Typography, Spin, message, Modal, Space, Empty, Popconfirm } from 'antd';
import { QrcodeOutlined, UserOutlined, ReloadOutlined, DeleteOutlined, ExclamationCircleOutlined } from '@ant-design/icons';
import { QRCodeSVG } from 'qrcode.react';
import type { Event } from '@tauri-apps/api/event';
import { useAuthStore } from '../stores/authStore';
import { useTauriListens } from '../utils/useTauriListens';
import { isSessionExpired, normalizeError } from '../utils/errors';
import type { QrCodeEvent, LoginSuccessEvent } from '../types';

const { Title, Text } = Typography;

export default function Login() {
  const navigate = useNavigate();
  // selector 细粒化：仅订阅用到的字段，避免无关字段变化导致重渲染
  const initClient = useAuthStore((s) => s.initClient);
  const fetchSavedUsers = useAuthStore((s) => s.fetchSavedUsers);
  const loginWithSession = useAuthStore((s) => s.loginWithSession);
  const removeSavedUser = useAuthStore((s) => s.removeSavedUser);
  const startQrLogin = useAuthStore((s) => s.startQrLogin);
  const savedUsers = useAuthStore((s) => s.savedUsers);
  const isLoggedIn = useAuthStore((s) => s.isLoggedIn);
  const userInfo = useAuthStore((s) => s.userInfo);

  const [qrUrl, setQrUrl] = useState<string | null>(null);
  const [qrLoading, setQrLoading] = useState(false);
  // 单个用户级别的加载状态，避免某个用户登录时所有按钮一起转圈
  const [loggingInUser, setLoggingInUser] = useState<string | null>(null);

  useEffect(() => {
    initClient().then(() => fetchSavedUsers());
  }, [initClient, fetchSavedUsers]);

  useEffect(() => {
    if (isLoggedIn) {
      navigate('/', { replace: true });
    }
  }, [isLoggedIn, navigate]);

  // 安全订阅 Tauri 事件
  useTauriListens([
    {
      event: 'qr-code',
      handler: (event: Event<unknown>) => {
        const payload = event.payload as QrCodeEvent;
        setQrUrl(payload.url);
        setQrLoading(false);
      },
    },
    {
      event: 'login-success',
      handler: (event: Event<unknown>) => {
        const payload = event.payload as LoginSuccessEvent;
        message.success(`登录成功: ${payload.name}`);
      },
    },
  ]);

  const handleQrLogin = useCallback(async () => {
    setQrLoading(true);
    setQrUrl(null);
    try {
      await startQrLogin();
    } catch (e) {
      setQrLoading(false);
      message.error(normalizeError(e).message);
    }
  }, [startQrLogin]);

  const handleSessionLogin = useCallback(
    async (username: string) => {
      setLoggingInUser(username);
      try {
        await loginWithSession(username);
        message.success(`已切换到用户: ${username}`);
      } catch (err) {
        const normalized = normalizeError(err);
        if (isSessionExpired(err)) {
          // 弹窗提示用户 cookie 已过期，引导其重新扫码登录
          Modal.confirm({
            title: '登录会话已过期',
            icon: <ExclamationCircleOutlined />,
            content: `账号 ${username} 的本地登录会话（Cookie）已失效，无法直接登录，请重新扫码登录。`,
            okText: '立即扫码登录',
            cancelText: '稍后',
            onOk: () => {
              handleQrLogin();
            },
          });
        } else {
          message.error(normalized.message || '登录失败，请稍后重试');
        }
      } finally {
        setLoggingInUser(null);
      }
    },
    [loginWithSession, handleQrLogin],
  );

  const handleRemoveSavedUser = useCallback(
    async (username: string) => {
      try {
        const deletingCurrentUser = isLoggedIn && username === userInfo?.name;
        await removeSavedUser(username);
        message.success(
          deletingCurrentUser
            ? `已删除本地保存账号 ${username}，并退出当前登录`
            : `已删除本地保存账号: ${username}`,
        );
      } catch (e) {
        message.error(`删除本地保存账号失败: ${normalizeError(e).message}`);
      }
    },
    [isLoggedIn, removeSavedUser, userInfo?.name],
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
                        loading={loggingInUser === user}
                        disabled={loggingInUser !== null && loggingInUser !== user}
                        onClick={() => handleSessionLogin(user)}
                      >
                        登录
                      </Button>,
                      <Popconfirm
                        title="确定删除该本地保存账号？"
                        description="仅删除本地保存的登录会话，不影响雨课堂平台账号本身。"
                        onConfirm={() => handleRemoveSavedUser(user)}
                        okText="确定删除"
                        cancelText="取消"
                        okButtonProps={{ danger: true }}
                      >
                        <Button type="link" size="small" danger icon={<DeleteOutlined />}>
                          删除
                        </Button>
                      </Popconfirm>,
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
                loading={qrLoading}
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
