export interface TerminalInfo {
  id: string;
  name: string;
}

export interface SessionMeta {
  sessionId: string;
  filePath: string;
  /** 会话发生的工作目录 */
  cwd: string;
  /** summary 优先,否则取第一条用户提问 */
  title: string;
  /** 用户侧消息数(真人提问,不含工具结果/子代理) */
  userMessageCount: number;
  /** 最后一条用户提问的时间(epoch 毫秒;无提问则回退为文件修改时间) */
  lastActive: number;
}
