import { Card, Typography, Descriptions, Tag, Space } from 'antd';
import { SettingOutlined, InfoCircleOutlined } from '@ant-design/icons';

const { Title } = Typography;

export default function Settings() {
  return (
    <div>
      <Title level={4} style={{ marginBottom: 16 }}>
        <SettingOutlined /> 设置
      </Title>

      <Space direction="vertical" style={{ width: '100%' }} size="large">
        <Card title={<><InfoCircleOutlined /> 关于</>}>
          <Descriptions column={1} bordered size="small">
            <Descriptions.Item label="应用名称">雨课堂助手</Descriptions.Item>
            <Descriptions.Item label="版本">v2.0.0</Descriptions.Item>
            <Descriptions.Item label="技术栈">
              <Space>
                <Tag color="orange">Rust</Tag>
                <Tag color="blue">React 18</Tag>
                <Tag color="cyan">Tauri v2</Tag>
                <Tag color="geekblue">Ant Design</Tag>
              </Space>
            </Descriptions.Item>
            <Descriptions.Item label="功能">
              课程查看、答案导出、自动刷课（视频/PPT/公告）、Excel 导出
            </Descriptions.Item>
            <Descriptions.Item label="说明">
              本工具仅用于个人学习与研究，请合理使用
            </Descriptions.Item>
          </Descriptions>
        </Card>

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
