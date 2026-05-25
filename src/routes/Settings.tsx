import { Card, Typography, Space } from 'antd';
import { SettingOutlined } from '@ant-design/icons';

const { Title } = Typography;

export default function Settings() {
  return (
    <div>
      <Title level={4} style={{ marginBottom: 16 }}>
        <SettingOutlined /> 设置
      </Title>

      <Space direction="vertical" style={{ width: '100%' }} size="large">
        <Card title="数据存储位置">
          <Typography.Paragraph type="secondary">
            所有数据（会话、答案文件、导出数据）均存储在系统应用数据目录中。
            <br />
            Windows: <code>%APPDATA%/com.raincourse.app/</code>
          </Typography.Paragraph>
        </Card>
      </Space>
    </div>
  );
}
