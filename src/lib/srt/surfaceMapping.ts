// Simplified Chinese → Japanese kanji (Shinjitai) mapping for drama proper nouns.
// Characters whose simplified form differs from Japanese kanji. Each key appears once.
const SIMPLIFIED_TO_JAPANESE: Record<string, string> = {
  "万": "萬", "与": "與", "专": "專", "业": "業", "丛": "叢",
  "东": "東", "丝": "絲", "两": "兩", "严": "嚴", "个": "個",
  "丰": "豐", "临": "臨", "为": "為", "丽": "麗", "举": "舉",
  "义": "義", "乐": "樂", "书": "書", "买": "買", "乱": "亂",
  "争": "爭", "亚": "亞", "产": "產", "亩": "畝", "亲": "親",
  "亿": "億", "仅": "僅", "仆": "僕", "从": "從", "仓": "倉",
  "仪": "儀", "众": "眾", "优": "優", "会": "會", "伟": "偉",
  "传": "傳", "伤": "傷", "伦": "倫", "伪": "偽", "侦": "偵",
  "侧": "側", "债": "債", "倾": "傾", "储": "儲", "儿": "兒",
  "兑": "兌", "兽": "獸", "冯": "馮", "冲": "衝", "决": "決",
  "冻": "凍", "净": "淨", "凄": "悽", "减": "減", "几": "幾",
  "击": "擊", "创": "創", "刘": "劉", "则": "則", "刚": "剛",
  "剧": "劇", "剑": "劍", "剂": "劑",
  "办": "辦", "协": "協", "单": "單", "华": "華", "卖": "賣",
  "卫": "衛", "卷": "捲", "厅": "廳", "历": "歷", "压": "壓",
  "厉": "厲", "厌": "厭", "县": "縣", "发": "發", "变": "變",
  "叠": "疊", "号": "號", "台": "臺", "叶": "葉",
  "后": "後", "吕": "呂", "吴": "吳", "员": "員",
  "园": "園", "围": "圍", "国": "國", "图": "図", "圆": "圓",
  "团": "團", "场": "場", "垒": "壘", "堕": "墮", "报": "報",
  "声": "聲", "壳": "殼", "壶": "壺", "处": "處", "备": "備",
  "复": "復", "头": "頭", "奖": "獎", "夺": "奪", "奋": "奮",
  "娇": "嬌", "孙": "孫", "学": "學", "实": "實", "审": "審",
  "写": "寫", "宽": "寬", "宝": "寶", "导": "導", "寿": "壽",
  "将": "將", "对": "對", "尔": "爾", "尘": "塵",
  "尝": "嘗", "岁": "歲", "岂": "豈", "岗": "崗", "岛": "島",
  "峡": "峽", "峰": "峯", "师": "師", "帐": "帳", "帘": "簾",
  "帮": "幫", "币": "幣",
  "庄": "莊", "庆": "慶", "应": "應", "广": "廣", "庙": "廟",
  "废": "廢", "庐": "廬", "库": "庫", "异": "異", "弃": "棄",
  "张": "張", "弹": "彈", "强": "強", "归": "歸", "录": "錄",
  "彻": "徹", "征": "徵", "径": "徑",
  "态": "態", "恳": "懇", "虑": "慮", "惩": "懲", "悬": "懸",
  "闷": "悶", "闻": "聞", "问": "問", "间": "間", "关": "關",
  "闪": "閃", "开": "開", "闲": "閒",
  "战": "戰", "戏": "戲", "户": "戶", "扑": "撲",
  "执": "執", "扩": "擴", "扫": "掃", "抚": "撫",
  "担": "擔", "拟": "擬", "拥": "擁", "拦": "攔",
  "拨": "撥", "择": "擇", "挥": "揮", "损": "損", "捡": "撿",
  "换": "換", "据": "據", "掺": "摻", "揽": "攬", "摆": "擺",
  "敛": "斂", "数": "數", "无": "無", "时": "時",
  "显": "顯", "晓": "曉", "昼": "晝", "暂": "暫", "旷": "曠",
  "术": "術", "杂": "雜", "权": "權", "条": "條", "来": "來",
  "极": "極", "构": "構", "标": "標", "树": "樹", "桥": "橋",
  "机": "機", "检": "檢", "栏": "欄", "樱": "櫻",
  "杀": "殺", "歼": "殲", "毁": "毀", "气": "氣",
  "汉": "漢", "沟": "溝", "沪": "滬", "沦": "淪", "泽": "澤",
  "洁": "潔", "浇": "澆", "测": "測", "济": "濟", "浓": "濃",
  "涛": "濤", "涤": "滌", "润": "潤", "涨": "漲", "渗": "滲",
  "溃": "潰", "溅": "濺", "湾": "灣", "灭": "滅", "灯": "燈",
  "灵": "靈", "炉": "爐", "炼": "煉", "烂": "爛", "热": "熱",
  "爷": "爺",
  "献": "獻", "狱": "獄", "环": "環", "画": "畫",
  "疗": "療", "疯": "瘋",
  "监": "監", "盘": "盤", "盖": "蓋", "盗": "盜", "尽": "盡",
  "盐": "鹽",
  "础": "礎", "确": "確", "碍": "礙", "礼": "禮", "祷": "禱",
  "离": "離", "种": "種", "积": "積", "称": "稱", "稳": "穩",
  "穷": "窮", "窃": "竊", "笔": "筆", "简": "簡", "签": "籤",
  "节": "節", "范": "範", "筑": "築",
  "类": "類", "粮": "糧", "粪": "糞",
  "纠": "糾", "纪": "紀", "约": "約", "纹": "紋", "纺": "紡",
  "罗": "羅", "罢": "罷", "罚": "罰",
  "职": "職", "联": "聯", "聪": "聰", "肃": "肅",
  "肠": "腸", "肤": "膚", "肿": "腫", "胜": "勝", "舆": "輿",
  "旧": "舊", "舍": "捨", "艺": "藝", "药": "薬", "获": "獲",
  "虚": "虛", "虏": "虜", "虫": "蟲",
  "虽": "雖", "蛮": "蠻",
  "衔": "銜", "补": "補", "装": "裝", "袭": "襲",
  "观": "觀", "规": "規", "视": "視", "览": "覽", "觉": "覺",
  "触": "觸",
  "训": "訓", "访": "訪", "设": "設", "许": "許", "诊": "診",
  "详": "詳", "诚": "誠", "诞": "誕", "询": "詢", "该": "該",
  "诱": "誘", "语": "語", "谁": "誰", "谅": "諒", "谋": "謀",
  "谦": "謙", "谨": "謹", "谬": "謬", "译": "譯",
  "话": "話", "说": "說", "讲": "講", "记": "記", "识": "識",
  "认": "認", "让": "讓", "议": "議", "论": "論", "证": "證",
  "试": "試", "诗": "詩", "误": "誤", "读": "讀", "课": "課",
  "调": "調", "谈": "談", "请": "請", "诺": "諾", "谓": "謂",
  "谣": "謠", "谢": "謝", "警": "警", "词": "詞", "评": "評",
  "诈": "詐", "诉": "訴",
  "负": "負", "贡": "貢", "财": "財", "责": "責", "贤": "賢",
  "败": "敗", "购": "購", "贮": "貯", "贯": "貫", "贱": "賤",
  "贴": "貼", "贺": "賀", "贾": "賈", "赁": "賃", "贿": "賄",
  "赃": "贓", "资": "資", "赈": "賑", "赌": "賭", "赔": "賠",
  "赖": "賴", "赚": "賺", "赞": "讚", "赠": "贈",
  "赵": "趙", "赶": "趕", "趋": "趨", "践": "踐", "踪": "蹤",
  "跃": "躍",
  "轩": "軒", "软": "軟", "轰": "轟", "轴": "軸", "轻": "輕",
  "载": "載", "辅": "輔", "辉": "輝", "辈": "輩", "辐": "輻",
  "辑": "輯", "输": "輸", "辕": "轅",
  "辞": "辭", "辩": "辯",
  "达": "達", "迁": "遷", "违": "違", "迟": "遲", "还": "還",
  "这": "這", "进": "進", "远": "遠", "连": "連", "选": "選",
  "逊": "遜", "递": "遞", "遥": "遙", "辽": "遼",
  "邓": "鄧", "邻": "鄰", "邮": "郵", "郑": "鄭", "邹": "鄒",
  "释": "釋", "阀": "閥", "阁": "閣", "阅": "閱",
  "阎": "閻", "阐": "闡", "阔": "闊",
  "队": "隊", "阶": "階", "际": "際", "陈": "陳",
  "陆": "陸", "险": "險", "随": "隨", "隐": "隱",
  "难": "難", "云": "雲", "雾": "霧", "静": "靜",
  "项": "項", "顺": "順", "须": "須", "顾": "顧", "顿": "頓",
  "预": "預", "领": "領", "颇": "頗", "频": "頻", "颓": "頹",
  "颗": "顆", "题": "題", "颜": "顏", "额": "額", "颠": "顛",
  "颤": "顫",
  "饥": "饑", "饭": "飯", "饮": "飲", "饲": "飼", "饱": "飽",
  "饰": "飾", "饺": "餃", "饼": "餅", "饿": "餓", "馆": "館",
  "馒": "饅",
  "驶": "駛", "驻": "駐", "驾": "駕", "骂": "罵",
  "骆": "駱", "骇": "駭", "骗": "騙", "骚": "騷", "验": "驗",
  "骄": "驕",
  "鲁": "魯", "鲜": "鮮", "鲤": "鯉", "鲸": "鯨", "鳞": "鱗",
  "鸡": "雞", "鸣": "鳴", "鸭": "鴨", "鸽": "鴿",
  "鹅": "鵝", "鹊": "鵲", "鹤": "鶴", "鹰": "鷹",
  "齿": "齒", "龄": "齡",
  "红": "紅", "绿": "綠", "线": "線", "组": "組", "织": "織",
  "结": "結", "给": "給", "经": "經", "绝": "絕", "统": "統",
  "编": "編", "练": "練", "维": "維", "缘": "緣", "纵": "縱",
  "纷": "紛", "纸": "紙", "纳": "納", "纯": "純",
  "细": "細", "绳": "繩", "绪": "緒", "续": "續", "绩": "績",
  "网": "網", "转": "轉", "轮": "輪", "较": "較",
  "军": "軍", "动": "動", "劳": "勞", "势": "勢", "励": "勵",
  "劲": "勁", "务": "務", "勋": "勳",
  "钢": "鋼", "铁": "鐵", "银": "銀", "铜": "銅", "镜": "鏡",
  "钱": "錢", "镇": "鎮", "钟": "鐘", "针": "針", "钉": "釘",
  "阳": "陽", "阴": "陰", "阵": "陣",
  "质": "質", "货": "貨", "贵": "貴", "费": "費", "赏": "賞",
  "门": "門", "马": "馬", "鱼": "魚", "鸟": "鳥", "龟": "龜",
  "龙": "龍", "飞": "飛", "风": "風", "电": "電", "韦": "韋",
  "页": "頁", "贝": "貝", "见": "見", "长": "長",
  // Specific Chinese drama proper noun chars
  "铲": "剷", "斩": "斬", "帅": "帥", "诸": "諸", "寻": "尋",
  "宫": "宮", "边": "邊", "遗": "遺", "怀": "懐",
};

/** Convert simplified Chinese characters in a string to Japanese kanji (Shinjitai). */
export function convertSimplifiedToJapanese(s: string): string {
  let result = "";
  for (const ch of s) {
    result += SIMPLIFIED_TO_JAPANESE[ch] ?? ch;
  }
  return result;
}

/** Transform kanji-converted Chinese compound into natural Japanese.
 *  Applied AFTER convertSimplifiedToJapanese so regexes match Japanese kanji. */
export function zhSurfaceToReadableJa(_sourceText: string, ja: string): string {
  let result = ja;

  // 兵馬 in military titles → 軍 (Chinese "兵馬" is too stiff for Japanese)
  result = result.replace(/兵馬(大元帥|大将軍|将軍|元帥|総督)/g, "軍$1");

  // Insert の between dynasty prefix and direction
  result = result.replace(/^(大[雍秦漢唐宋明清隋元魏])([東西南北中])/, "$1の$2");

  // X宮Y祖廟/X宮Y殿 → X宮のY…
  result = result.replace(/(宮)([承光祖宗天太永昭神竜龍玄霊])/g, "$1の$2");

  return result;
}

/** Map English category words in source_text to Japanese equivalents on the kanji surface.
 *  Example: "Gelin tribe" + zh="哥林部" + ja="哥林部" → "哥林族" */
export function applyCategorySuffixMapping(sourceText: string, zh: string, ja: string): string {
  const source = sourceText.trim().toLowerCase();

  // tribe → 族 (Chinese "部" is a direct translation of "tribe" but Japanese prefers "族")
  if (/\btribe\b/.test(source)) {
    if (/^[一-鿿々]{1,8}部$/.test(ja)) {
      return ja.replace(/部$/, "族");
    }
    if (/^[一-鿿]{1,8}部$/.test(zh)) {
      return convertSimplifiedToJapanese(zh).replace(/部$/, "族");
    }
  }

  return ja;
}

/** Map English honorifics/titles in source_text to natural Japanese equivalents.
 *  Applied AFTER convertSimplifiedToJapanese + zhSurfaceToReadableJa + applyCategorySuffixMapping.
 *  zh_surface is kept as evidence; only surface_ja / confirmed_surface is transformed. */
export function applyHonorificMapping(
  sourceText: string,
  zh: string,
  ja: string,
): string {
  const source = sourceText.trim();
  const zhTrimmed = zh.trim();

  // Lord <Name> + 大人 → <Name>卿
  if (/^lord\s+/i.test(source) && /大人$/.test(zhTrimmed)) {
    const baseZh = zhTrimmed.replace(/大人$/, "");
    const baseJa = convertSimplifiedToJapanese(baseZh);
    return `${baseJa}卿`;
  }

  // Commander <Name> + 统领/統領 → <Name>統領
  if (/^commander\s+/i.test(source) && /(统领|統領)$/.test(zhTrimmed)) {
    const baseZh = zhTrimmed.replace(/(统领|統領)$/, "");
    const baseJa = convertSimplifiedToJapanese(baseZh);
    return `${baseJa}統領`;
  }

  // General <Name> + 将军/將軍/将軍 → <Name>将軍
  if (/^general\s+/i.test(source) && /(将军|將軍|将軍)$/.test(zhTrimmed)) {
    const baseZh = zhTrimmed.replace(/(将军|將軍|将軍)$/, "");
    const baseJa = convertSimplifiedToJapanese(baseZh);
    return `${baseJa}将軍`;
  }

  // Young Master <Name> + 少爷/少爺 → <Name>若様
  if (/^young master\s+/i.test(source) && /(少爷|少爺)$/.test(zhTrimmed)) {
    const baseZh = zhTrimmed.replace(/(少爷|少爺)$/, "");
    const baseJa = convertSimplifiedToJapanese(baseZh);
    return `${baseJa}若様`;
  }

  // Lady <Name> + 小姐 → <Name>嬢
  if (/^lady\s+/i.test(source) && /小姐$/.test(zhTrimmed)) {
    const baseZh = zhTrimmed.replace(/小姐$/, "");
    const baseJa = convertSimplifiedToJapanese(baseZh);
    return `${baseJa}嬢`;
  }

  // Lady <Name> + 夫人 → <Name>夫人（簡体字→日本語漢字のみ）
  if (/^lady\s+/i.test(source) && /夫人$/.test(zhTrimmed)) {
    return convertSimplifiedToJapanese(zhTrimmed);
  }

  // 皇上単独 → 陛下
  if (zhTrimmed === "皇上") {
    return "陛下";
  }

  return ja;
}

/** Compose the full 4-stage pipeline from Chinese zh_surface to natural Japanese surface. */
export function buildJapaneseSurfaceFromZh(sourceText: string, zhSurface: string): string {
  const jaBase = convertSimplifiedToJapanese(zhSurface);
  const jaReadable = zhSurfaceToReadableJa(sourceText, jaBase);
  const jaCategory = applyCategorySuffixMapping(sourceText, zhSurface, jaReadable);
  return applyHonorificMapping(sourceText, zhSurface, jaCategory);
}

export function normalizeSurfaceForCompare(s: string): string {
  return s
    .trim()
    .replace(/\s+/g, "")
    .replace(/[・･]/g, "")
    .replace(/[ー－ｰ]/g, "ー");
}

/** Detect whether the current surface_ja / confirmed_surface value is
 *  an auto-generated pipeline output (safe to overwrite) or a manual edit
 *  (must preserve).  Returns true for auto-generated values. */
export function looksLikeAutoGeneratedSurface(
  sourceText: string,
  zhSurface: string,
  currentSurface?: string | null,
): boolean {
  const current = currentSurface?.trim();
  if (!current || current === "-") return true;

  const jaBase = convertSimplifiedToJapanese(zhSurface);
  const jaReadable = zhSurfaceToReadableJa(sourceText, jaBase);
  const jaCategory = applyCategorySuffixMapping(sourceText, zhSurface, jaReadable);
  const jaHonorific = applyHonorificMapping(sourceText, zhSurface, jaCategory);

  const currentNorm = normalizeSurfaceForCompare(current);

  const autoCandidates = [
    zhSurface,
    convertSimplifiedToJapanese(zhSurface),
    jaBase,
    jaReadable,
    jaCategory,
    jaHonorific,
  ].map(normalizeSurfaceForCompare);

  if (autoCandidates.includes(currentNorm)) return true;

  // Old pipeline produced Lord ... 大人; safe to upgrade to 卿
  if (/^lord\s+/i.test(sourceText) && /大人$/.test(current) && /大人$/.test(zhSurface)) return true;

  // Old pipeline produced Young Master ... 少爺; safe to upgrade to 若様
  if (/^young master\s+/i.test(sourceText) && /(少爺|少爷)$/.test(current) && /(少爺|少爷)$/.test(zhSurface)) return true;

  return false;
}

export type HonorificSurfaceTerm = {
  source_text: string;
  zh_surface?: string;
  surface_ja?: string;
  confirmed_surface?: string;
};

/** Pure function encapsulating Phase 3 surface update logic.
 *  Given a term and a zhSurface, computes jaGenerated via the full pipeline
 *  and decides whether to overwrite surface_ja / confirmed_surface. */
export function applyZhSurfaceToTerm(
  term: HonorificSurfaceTerm,
  zhSurface: string,
): HonorificSurfaceTerm {
  const jaGenerated = buildJapaneseSurfaceFromZh(term.source_text, zhSurface);

  const shouldUpdateSurface = looksLikeAutoGeneratedSurface(
    term.source_text,
    zhSurface,
    term.surface_ja,
  );

  const shouldUpdateConfirmed = looksLikeAutoGeneratedSurface(
    term.source_text,
    zhSurface,
    term.confirmed_surface || term.surface_ja,
  );

  return {
    ...term,
    zh_surface: zhSurface,
    surface_ja: shouldUpdateSurface ? jaGenerated : term.surface_ja,
    confirmed_surface: shouldUpdateConfirmed
      ? jaGenerated
      : (term.confirmed_surface || term.surface_ja || jaGenerated),
  };
}
