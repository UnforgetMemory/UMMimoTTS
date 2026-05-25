export const SAMPLE_TXT_CONTENT = `这是一段用于测试TTS语音合成的示例文本。
它包含多个段落，用于验证批量导入功能能否正确解析文件内容。

第二段内容，包含一些中文标点符号：逗号，句号。问号？感叹号！
第三段：数字和字母混合测试 12345 ABCDE abcde

第四段——长文本测试：
在很久很久以前，有一座古老的城堡坐落在一片茂密的森林中央。
城堡里住着一位善良的公主，她有着一头乌黑亮丽的长发。
每天清晨，她都会在城堡的花园里散步，聆听鸟儿的歌唱。
森林深处有一口神秘的许愿井，据说只要向井里投下一枚硬币，
许下的愿望就一定会实现。公主经常来到这里，许愿王国永远和平。`;

/**
 * Generate a test file with known content.
 * @param filename - The name of the file (should end with .txt)
 * @param lines - Number of lines to include (defaults to all)
 */
export function generateTestFile(
  filename: string,
  lines?: number,
): { name: string; mimeType: string; buffer: Buffer } {
  let content = SAMPLE_TXT_CONTENT;
  if (lines !== undefined) {
    content = content
      .split('\n')
      .slice(0, lines)
      .join('\n');
  }
  return {
    name: filename,
    mimeType: 'text/plain',
    buffer: Buffer.from(content, 'utf-8'),
  };
}

/**
 * Generate multiple test files.
 */
export function generateMultipleFiles(
  count: number,
): Array<{ name: string; mimeType: string; buffer: Buffer }> {
  const files: Array<{ name: string; mimeType: string; buffer: Buffer }> = [];
  for (let i = 0; i < count; i++) {
    files.push(
      generateTestFile(`test-${i + 1}.txt`, 3),
    );
  }
  return files;
}

/**
 * Generate a non-txt file for invalid file type tests.
 */
export function generateNonTextFile(): { name: string; mimeType: string; buffer: Buffer } {
  return {
    name: 'document.pdf',
    mimeType: 'application/pdf',
    buffer: Buffer.from('%PDF-1.4 fake pdf content', 'utf-8'),
  };
}
