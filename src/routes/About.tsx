import { Card, Typography, Space, Tag, Descriptions, Alert, Divider } from 'antd';
import {
  InfoCircleOutlined,
  GithubOutlined,
  UserOutlined,
  TeamOutlined,
  WarningOutlined,
  FileProtectOutlined,
} from '@ant-design/icons';
import { open as openExternal } from '@tauri-apps/plugin-shell';
import type { MouseEvent } from 'react';
import PageHeader from '../components/PageHeader';

const { Paragraph, Text, Link } = Typography;

const REPO_URL = 'https://github.com/hor1zon777/Raincourse';
const ORIGINAL_REPO_URL = 'https://github.com/aglorice/Raincourse';
const GPL3_URL = 'https://www.gnu.org/licenses/gpl-3.0.html';

/**
 * 用 Tauri shell.open 在系统默认浏览器打开 URL。
 *
 * 不用普通 `<a target="_blank">`：Tauri webview 默认会在应用窗口内导航，
 * 把用户从单页应用里挤掉。
 */
function useExternalLink(url: string) {
  return (e: MouseEvent<HTMLElement>) => {
    e.preventDefault();
    openExternal(url).catch((err) => {
      console.error('打开外链失败:', url, err);
    });
  };
}

function ExtLink({ url, children }: { url: string; children: React.ReactNode }) {
  const onClick = useExternalLink(url);
  return (
    <Link href={url} onClick={onClick}>
      {children}
    </Link>
  );
}

export default function About() {
  return (
    <div>
      <PageHeader icon={<InfoCircleOutlined />} title="关于" />

      <Space direction="vertical" style={{ width: '100%' }} size="large">
        <Card title={<><InfoCircleOutlined /> 应用信息</>}>
          <Descriptions column={1} bordered size="small">
            <Descriptions.Item label="应用名称">雨课堂助手</Descriptions.Item>
            <Descriptions.Item label="版本">v2.0.0</Descriptions.Item>
            <Descriptions.Item label="技术栈">
              <Space wrap>
                <Tag color="orange">Rust</Tag>
                <Tag color="blue">React 19</Tag>
                <Tag color="cyan">Tauri v2</Tag>
                <Tag color="geekblue">Ant Design</Tag>
              </Space>
            </Descriptions.Item>
          </Descriptions>
        </Card>

        <Card title={<><UserOutlined /> 作者</>}>
          <Descriptions column={1} bordered size="small">
            <Descriptions.Item label="维护者">hor1zon777</Descriptions.Item>
            <Descriptions.Item label="项目地址">
              <ExtLink url={REPO_URL}>
                <GithubOutlined /> {REPO_URL}
              </ExtLink>
            </Descriptions.Item>
          </Descriptions>
        </Card>

        <Card title={<><TeamOutlined /> 原作者 / 致谢</>}>
          <Paragraph>
            本项目基于{' '}
            <Text strong>aglorice</Text>{' '}
            的开源工作进行二次开发，特此致谢。
          </Paragraph>
          <Descriptions column={1} bordered size="small">
            <Descriptions.Item label="原作者">aglorice</Descriptions.Item>
            <Descriptions.Item label="原项目地址">
              <ExtLink url={ORIGINAL_REPO_URL}>
                <GithubOutlined /> {ORIGINAL_REPO_URL}
              </ExtLink>
            </Descriptions.Item>
          </Descriptions>
        </Card>

        <Card title={<><WarningOutlined /> 免责声明</>}>
          <Alert
            type="warning"
            showIcon
            message="请在使用本工具前阅读并理解以下声明"
            style={{ marginBottom: 16 }}
          />
          <Paragraph>
            1. 本工具<Text strong>仅供个人学习、研究与技术交流使用</Text>，请勿用于任何商业用途或非法用途。
          </Paragraph>
          <Paragraph>
            2. 使用本工具产生的所有行为（包括但不限于账号封禁、成绩异常、纪律处分、法律责任等）<Text strong>均由使用者自行承担</Text>，与本项目作者及贡献者无关。
          </Paragraph>
          <Paragraph>
            3. 本工具与雨课堂、学堂在线及其运营方<Text strong>无任何隶属关系</Text>，本项目不对工具的可用性、稳定性及数据准确性作出任何承诺或担保。
          </Paragraph>
          <Paragraph>
            4. 使用本工具即表示您<Text strong>已知悉并接受</Text>上述全部条款。如不同意，请立即停止使用并删除本软件。
          </Paragraph>
          <Paragraph>
            5. 请遵守所在国家 / 地区的法律法规及目标平台的用户协议。任何违反法律法规或平台协议的行为均与本项目无关。
          </Paragraph>
        </Card>

        <Card title={<><FileProtectOutlined /> 开源协议</>}>
          <Paragraph>
            本项目遵循 <Text strong>GNU General Public License v3.0 (GPL-3.0)</Text> 协议开源发布。
          </Paragraph>
          <Paragraph type="secondary" style={{ marginBottom: 8 }}>
            <Text strong>您可以：</Text>
          </Paragraph>
          <Paragraph>
            ✓ 自由使用、复制、修改和分发本软件<br />
            ✓ 将本软件用于个人与商业目的<br />
            ✓ 基于本软件构建衍生作品
          </Paragraph>
          <Divider style={{ margin: '12px 0' }} />
          <Paragraph type="secondary" style={{ marginBottom: 8 }}>
            <Text strong>但您必须：</Text>
          </Paragraph>
          <Paragraph>
            ✓ 保留原始版权声明、许可证文本与免责声明<br />
            ✓ 任何衍生作品同样以 GPL-3.0 协议开源（Copyleft 传染性）<br />
            ✓ 在分发时附带完整源代码或提供源代码获取方式<br />
            ✓ 明确标注对原始代码所做的修改
          </Paragraph>
          <Divider style={{ margin: '12px 0' }} />
          <Paragraph type="secondary">
            本软件按"原样"提供，不附带任何明示或暗示的担保，包括但不限于适销性、特定用途适用性的担保。
            完整协议文本请参阅：{' '}
            <ExtLink url={GPL3_URL}>{GPL3_URL}</ExtLink>
          </Paragraph>
        </Card>
      </Space>
    </div>
  );
}
