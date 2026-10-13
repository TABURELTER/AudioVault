import type { DuplicatePair } from "./types";
export const issueLabels: Record<string, string> = {
  unknown_author: "Автор не указан",
  missing_title: "Проверьте название",
  transliterated_folder: "Название латиницей",
  release_garbage: "Технические сведения в названии",
  tracks_disagree: "Теги глав различаются",
  fb2_tag_conflict: "Конфликт FB2 и аудиотегов",
  missing_narrator: "Чтец не указан",
  series_numbering: "Порядок книг в серии",
  probe_errors: "Файлы не читаются",
};
export const issueDescriptions: Record<string, string> = {
  unknown_author: "В тегах, FB2 и названии папки не найден автор.",
  missing_title:
    "Название определено по папке с низкой уверенностью. Проверьте его.",
  transliterated_folder:
    "Название папки записано латиницей. Проверьте предложенный вариант.",
  release_garbage: "Название содержит технические сведения о релизе.",
  tracks_disagree: "У глав этой записи различаются теги альбома.",
  fb2_tag_conflict: "Метаданные FB2 и аудиофайлов не совпадают.",
  missing_narrator: "В копиях этой книги не найдено имя чтеца.",
  series_numbering: "Номер книги не согласуется с другими данными о серии.",
  probe_errors:
    "Не удалось прочитать часть аудиофайлов. Проверьте исходную запись.",
};
const verdicts: Record<string, string> = {
  "Byte-identical copy": "Идентичные файлы",
  "Same recording": "Одна запись",
  "Possible duplicate": "Возможный дубликат",
  "Same book, different narrator": "Другой чтец",
  Unrelated: "Разные издания",
};
export function duplicateVerdict(value: string) {
  return verdicts[value] || value;
}
export function duplicateEvidence(pair: DuplicatePair) {
  const text = pair.evidence;
  const reasons: string[] = [];
  if (/byte-identical/i.test(text))
    reasons.push("Содержимое аудиофайлов совпадает.");
  if (/acoustic fingerprints match/i.test(text))
    reasons.push("Совпадают аудиоотпечатки записей.");
  if (/same canonical book/i.test(text)) reasons.push("Одна книга в каталоге.");
  else if (/same canonical title and author/i.test(text))
    reasons.push("Совпадают название и автор.");
  else if (/same normalized title/i.test(text))
    reasons.push("Совпадают названия; авторы различаются или не указаны.");
  const difference = text.match(/duration differs by ([\d.]+)%/i);
  if (difference)
    reasons.push(
      `Длительность отличается на ${Number(difference[1]).toLocaleString("ru-RU")}%.`,
    );
  if (/narrators differ/i.test(text)) reasons.push("Указаны разные чтецы.");
  if (/narrator unknown/i.test(text))
    reasons.push("У одной из копий не указан чтец.");
  if (!reasons.length && /[а-яё]/i.test(text)) {
    return text;
  }
  return (
    reasons.join(" ") ||
    "Обнаружено сходство записей. Сравните чтеца, длительность и расположение обеих копий."
  );
}
export const taskLabels: Record<string, string> = {
  scan: "Сканирование коллекции",
  metadata: "Чтение метаданных",
  quick_hash: "Быстрая проверка файлов",
  full_hash: "Проверка содержимого файлов",
  fingerprint: "Анализ аудиоотпечатков",
  duplicates: "Поиск дубликатов",
  thumbnails: "Подготовка обложек",
  file_operation: "Операция с файлами",
  transcode: "Конвертация аудио",
  torrent_import: "Импорт загруженных книг",
  reconcile: "Проверка операций после перезапуска",
  optimize_analysis: "Анализ экономии места",
};
