/**
 * Large-scale text data generators for E2E testing.
 *
 * Produces realistic Chinese + English mixed text files with configurable
 * segment counts and character lengths. Segments are separated by blank lines,
 * matching BatchImportWizard's `parseTextIntoSegments()` split logic.
 */

// ── Chinese phrase bank (realistic TTS content) ────────────────────────

const CHINESE_PHRASES = [
  '今天天气非常好，适合出去散步。阳光明媚，微风拂面，让人心情愉悦。',
  '科技的发展日新月异，人工智能正在改变我们的生活方式。从智能家居到自动驾驶，技术的进步无处不在。',
  '中国传统文化源远流长，蕴含着丰富的哲学思想和人生智慧。孔子的仁义礼智信至今仍有深远影响。',
  '春天来了，万物复苏，大地又恢复了生机勃勃的景象。桃花盛开，柳枝吐绿，一片欣欣向荣。',
  '音乐是人类共同的语言，能够跨越国界传递情感与力量。无论是古典还是现代，音乐总能触动人心。',
  '在数字化转型的浪潮中，企业面临着前所未有的机遇与挑战。创新是制胜的关键。',
  '读书使人明智，运动使人健康，两者缺一不可。良好的习惯是成功的基石。',
  '随着互联网技术的普及，信息获取变得更加便捷和高效。知识触手可及。',
  '环境保护是全人类共同的课题，需要每个人的参与和努力。绿水青山就是金山银山。',
  '创新是推动社会进步的重要动力，我们应该鼓励大胆探索和尝试。失败是成功之母。',
  '家庭是社会的基本单位，和谐的家庭关系对个人成长至关重要。家和万事兴。',
  '教育的本质不仅是传授知识，更是培养学生的思维能力和创造力。因材施教，有教无类。',
  '在全球化的背景下，跨文化交流和理解变得越来越重要。海纳百川，有容乃大。',
  '健康管理应该成为每个人的日常习惯，预防胜于治疗。身体是革命的本钱。',
  '城市规划需要兼顾经济发展和生态环境保护，实现可持续增长。人与自然和谐共生。',
  '人工智能语音合成技术正在快速发展，自然度和表现力不断提升。',
  '深度学习模型能够生成接近真人水平的语音，为内容创作带来革命性变化。',
  '文本转语音技术在有声书制作、播客创作、辅助教育等领域有广泛应用。',
  '高质量的语音合成需要考虑韵律、语调、情感表达等多个维度。',
  '批量处理大量文本时，系统的稳定性和处理效率是关键指标。',
];

const ENGLISH_PHRASES = [
  'The advancement of technology has revolutionized how we communicate and share information across the globe.',
  'Scientific research continues to push the boundaries of human knowledge and understanding in remarkable ways.',
  'Machine learning algorithms are being applied to solve complex problems in healthcare, finance, and education.',
  'The importance of sustainable development cannot be overstated in our rapidly changing modern world.',
  'Digital transformation is reshaping industries and creating new opportunities for innovation and growth.',
  'Cloud computing has enabled businesses to scale their operations more efficiently than ever before.',
  'The intersection of art and technology creates fascinating new possibilities for creative expression.',
  'Environmental conservation efforts require collaboration between governments, organizations, and communities.',
  'Natural language processing has made significant strides in understanding and generating human language.',
  'Text-to-speech synthesis technology continues to improve in naturalness and expressiveness.',
];

// ── Helpers ─────────────────────────────────────────────────────────────

/** Deterministic pseudo-random number generator (xorshift) */
function xorshift(seed: number): () => number {
  let state = seed || 1;
  return () => {
    state ^= state << 13;
    state ^= state >> 17;
    state ^= state << 5;
    return (state >>> 0) / 4294967296;
  };
}

/** Generate a single text segment of approximately `targetChars` characters */
function generateSegment(targetChars: number, rng: () => number): string {
  let text = '';
  while (text.length < targetChars) {
    if (rng() < 0.7) {
      // 70% Chinese
      text += CHINESE_PHRASES[Math.floor(rng() * CHINESE_PHRASES.length)];
    } else {
      // 30% English
      text += ENGLISH_PHRASES[Math.floor(rng() * ENGLISH_PHRASES.length)];
      text += ' ';
    }
  }
  // Trim to approximate target (character-level)
  const chars = [...text];
  if (chars.length > targetChars) {
    text = chars.slice(0, targetChars).join('');
  }
  return text;
}

// ── Public API ──────────────────────────────────────────────────────────

export interface LargeTextFile {
  name: string;
  mimeType: string;
  buffer: Buffer;
  segmentCount: number;
  totalChars: number;
}

/**
 * Generate a single large text file with N segments separated by blank lines.
 *
 * @param segmentCount - Number of text segments (default: 120)
 * @param charsPerSegment - Approximate chars per segment (default: 200)
 * @param filename - Output filename (default: 'large-text.txt')
 * @param seed - RNG seed for reproducibility (default: 42)
 */
export function generateLargeTextFile(
  segmentCount = 120,
  charsPerSegment = 200,
  filename = 'large-text.txt',
  seed = 42,
): LargeTextFile {
  const rng = xorshift(seed);
  const segments: string[] = [];

  for (let i = 0; i < segmentCount; i++) {
    // Vary segment length ±30%
    const variation = 0.7 + rng() * 0.6;
    const targetChars = Math.round(charsPerSegment * variation);
    const seg = generateSegment(targetChars, rng);
    segments.push(seg);
  }

  const content = segments.join('\n\n');
  const totalChars = content.length;

  return {
    name: filename,
    mimeType: 'text/plain',
    buffer: Buffer.from(content, 'utf-8'),
    segmentCount,
    totalChars,
  };
}

/**
 * Generate multiple large text files.
 *
 * @param fileCount - Number of files (default: 5)
 * @param segmentsPerFile - Segments per file (default: 25)
 * @param charsPerSegment - Approximate chars per segment (default: 200)
 */
export function generateMultipleLargeFiles(
  fileCount = 5,
  segmentsPerFile = 25,
  charsPerSegment = 200,
): LargeTextFile[] {
  const files: LargeTextFile[] = [];
  for (let i = 0; i < fileCount; i++) {
    files.push(
      generateLargeTextFile(
        segmentsPerFile,
        charsPerSegment,
        `novel-chapter-${String(i + 1).padStart(2, '0')}.txt`,
        42 + i * 1000,
      ),
    );
  }
  return files;
}

/**
 * Generate a single massive text file (200+ segments) for stress testing.
 */
export function generateMassiveTextFile(
  segmentCount = 200,
  charsPerSegment = 150,
): LargeTextFile {
  return generateLargeTextFile(segmentCount, charsPerSegment, 'massive-stress-test.txt', 999);
}

/**
 * Generate N independent files, each containing a single segment of
 * random length between minChars and maxChars.
 *
 * @param fileCount - Number of files to generate (default: 1000)
 * @param minChars  - Minimum characters per file (default: 1)
 * @param maxChars  - Maximum characters per file (default: 10000)
 */
export function generateManyFiles(
  fileCount = 1000,
  minChars = 1,
  maxChars = 10000,
): LargeTextFile[] {
  const rng = xorshift(12345);
  const files: LargeTextFile[] = [];

  for (let i = 0; i < fileCount; i++) {
    // Random char count in [minChars, maxChars]
    const charCount = Math.max(minChars, Math.round(minChars + rng() * (maxChars - minChars)));
    const text = generateSegment(charCount, rng);
    const idx = String(i + 1).padStart(4, '0');
    files.push({
      name: `stress-${idx}.txt`,
      mimeType: 'text/plain',
      buffer: Buffer.from(text, 'utf-8'),
      segmentCount: 1, // single segment per file (no blank-line splits)
      totalChars: text.length,
    });
  }

  return files;
}

/**
 * Summary helper — log total segments and chars across files.
 */
export function summarizeFiles(files: LargeTextFile[]): { totalSegments: number; totalChars: number } {
  const totalSegments = files.reduce((s, f) => s + f.segmentCount, 0);
  const totalChars = files.reduce((s, f) => s + f.totalChars, 0);
  return { totalSegments, totalChars };
}
