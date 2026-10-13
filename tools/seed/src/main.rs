//! Seed utility: populates an AudioVault catalog with realistic synthetic data
//! (Russian and English audiobooks, multiple volumes, variants, duplicates,
//! and metadata issues) to stress-test search, pagination, and UI performance.

use std::path::PathBuf;

use audiovault_core::duplicate::{Assessment, Evidence};
use audiovault_core::model::{CopyTech, DuplicateVerdict, IssueKind, StorageClass};
use audiovault_core::optimize::{ConversionKind, Recommendation, TargetFormat};
use audiovault_db::catalog::{self, CopyEnumeration, MediaEnumeration, ProbeUpdate};
use audiovault_db::duplicates;
use audiovault_db::issues::{self, NewIssue};
use audiovault_db::optimize;
use audiovault_db::volumes::{self, ObservedVolume, VolumeIdentity};
use audiovault_db::{Db, now_ts};

const AUTHORS: &[(&str, &[&str])] = &[
    (
        "Борис Акунин",
        &[
            "Азазель",
            "Турецкий гамбит",
            "Левиафан",
            "Смерть Ахиллеса",
            "Особые поручения",
            "Статский советник",
            "Коронация",
            "Любовница смерти",
            "Любовник смерти",
            "Алмазная колесница",
        ],
    ),
    (
        "Дмитрий Глуховский",
        &[
            "Метро 2033",
            "Метро 2034",
            "Метро 2035",
            "Будущее",
            "Текст",
            "Пост",
            "Сумерки",
            "Рассказы о Родине",
        ],
    ),
    (
        "Сергей Лукьяненко",
        &[
            "Ночной Дозор",
            "Дневной Дозор",
            "Сумеречный Дозор",
            "Последний Дозор",
            "Новый Дозор",
            "Шестой Дозор",
            "Черновик",
            "Чистовик",
            "Спектр",
            "Лабиринт отражений",
            "Фальшивые зеркала",
            "Прозрачные витражи",
        ],
    ),
    (
        "Аркадий и Борис Стругацкие",
        &[
            "Трудно быть богом",
            "Понедельник начинается в субботу",
            "Пикник на обочине",
            "Обитаемый остров",
            "Жук в муравейнике",
            "Волны гасят ветер",
            "Град обреченный",
            "Улитка на склоне",
            "Отель У Погибшего Альпиниста",
        ],
    ),
    (
        "Федор Достоевский",
        &[
            "Преступление и наказание",
            "Идиот",
            "Братья Карамазовы",
            "Бесы",
            "Игрок",
            "Белые ночи",
            "Записки из подполья",
            "Подросток",
        ],
    ),
    (
        "Лев Толстой",
        &[
            "Война и мир",
            "Анна Каренина",
            "Воскресение",
            "Смерть Ивана Ильича",
            "Крейцерова соната",
            "Казаки",
            "Хаджи-Мурат",
            "Детство",
            "Отрочество",
            "Юность",
        ],
    ),
    (
        "Михаил Булгаков",
        &[
            "Мастер и Маргарита",
            "Белая гвардия",
            "Собачье сердце",
            "Роковые яйца",
            "Записки юного врача",
            "Морфий",
            "Театральный роман",
            "Жизнь господина де Мольера",
        ],
    ),
    (
        "Виктор Пелевин",
        &[
            "Generation П",
            "Чапаев и Пустота",
            "Омон Ра",
            "Священная книга оборотня",
            "Empire V",
            "Бэтман Аполло",
            "iPhuck 10",
            "Трансгуманизм Inc.",
            "KGBT+",
        ],
    ),
    (
        "Дж. Р. Р. Толкин",
        &[
            "Властелин Колец: Братство Кольца",
            "Властелин Колец: Две крепости",
            "Властелин Колец: Возвращение короля",
            "Хоббит, или Туда и обратно",
            "Сильмариллион",
            "Дети Хурина",
            "Падение Гондолина",
        ],
    ),
    (
        "Джордж Оруэлл",
        &[
            "1984",
            "Скотный двор",
            "Памяти Каталонии",
            "Дни в Бирме",
            "Да здравствует фикус!",
            "Дорога на Уиган-Пирс",
        ],
    ),
    (
        "Агата Кристи",
        &[
            "Десять негритят",
            "Убийство в Восточном экспрессе",
            "Убийство Роджера Экройда",
            "Смерть на Ниле",
            "Загадка Эндхауза",
            "Убийства по алфавиту",
            "Свидание со смертью",
        ],
    ),
    (
        "Артур Конан Дойл",
        &[
            "Этюд в багровых тонах",
            "Знак четырех",
            "Собака Баскервилей",
            "Долина страха",
            "Приключения Шерлока Холмса",
            "Записки о Шерлоке Холмсе",
            "Возвращение Шерлока Холмса",
        ],
    ),
    (
        "Стивен Кинг",
        &[
            "Сияние",
            "Оно",
            "Зеленая миля",
            "Побег из Шоушенка",
            "Мизери",
            "Кладбище домашних животных",
            "Темная Башня: Стрелок",
            "Темная Башня: Извлечение троих",
            "11/22/63",
        ],
    ),
    (
        "Анджей Сапковский",
        &[
            "Последнее желание",
            "Меч Предназначения",
            "Кровь эльфов",
            "Час Презрения",
            "Крещение огнем",
            "Башня Ласточки",
            "Владычица Озера",
            "Сезон гроз",
        ],
    ),
    (
        "Станислав Лем",
        &[
            "Солярис",
            "Непобедимый",
            "Кибериада",
            "Звездные дневники Ийона Тихого",
            "Эдем",
            "Возвращение со звезд",
            "Магелланово Облако",
            "Фиаско",
        ],
    ),
];

const NARRATORS: &[&str] = &[
    "Александр Клюквин",
    "Сергей Чонишвили",
    "Игорь Князев",
    "Владимир Левашев",
    "Вячеслав Герасимов",
    "Семен Ярмолинец",
    "Иван Литвинов",
    "Максим Суханов",
    "Михаил Горевой",
    "Алексей Багдасаров",
    "Евгений Терновский",
    "Юрий Заборовский",
];

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let db_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("catalog_seed.sqlite")
    };

    println!("Seeding synthetic audiobooks into {}...", db_path.display());
    let db = Db::open(&db_path)?;

    let (v_internal, v_external, v_archive) = {
        let conn = db.conn()?;
        let v1 = volumes::insert(
            &conn,
            &ObservedVolume {
                identity: VolumeIdentity {
                    platform_id: Some("synth:nvme".into()),
                    serial: Some("NVME-001".into()),
                    marker_uuid: None,
                },
                label: Some("Fast Internal NVMe".into()),
                fs_type: Some("NTFS".into()),
                capacity_bytes: Some(1_000_000_000_000),
                free_bytes: Some(350_000_000_000),
                mount_path: "C:/Audiobooks".into(),
                storage_class: StorageClass::Ssd,
            },
            "Fast Internal NVMe",
        )?;
        let v2 = volumes::insert(
            &conn,
            &ObservedVolume {
                identity: VolumeIdentity {
                    platform_id: Some("synth:hdd-ext".into()),
                    serial: Some("WD-889922".into()),
                    marker_uuid: None,
                },
                label: Some("Audio Archive 4TB".into()),
                fs_type: Some("NTFS".into()),
                capacity_bytes: Some(4_000_000_000_000),
                free_bytes: Some(1_200_000_000_000),
                mount_path: "D:/Audiobooks".into(),
                storage_class: StorageClass::Hdd,
            },
            "Audio Archive 4TB",
        )?;
        let v3 = volumes::insert(
            &conn,
            &ObservedVolume {
                identity: VolumeIdentity {
                    platform_id: Some("synth:usb-flash".into()),
                    serial: Some("SANDISK-32".into()),
                    marker_uuid: None,
                },
                label: Some("USB Flash 32GB".into()),
                fs_type: Some("exFAT".into()),
                capacity_bytes: Some(32_000_000_000),
                free_bytes: Some(8_000_000_000),
                mount_path: "E:/Audiobooks".into(),
                storage_class: StorageClass::Removable,
            },
            "USB Flash 32GB",
        )?;
        volumes::add_root(&conn, v1, "")?;
        volumes::add_root(&conn, v2, "Library")?;
        volumes::add_root(&conn, v3, "Selected")?;
        (v1, v2, v3)
    };

    let mut book_count = 0;
    let mut copy_count = 0;

    db.write(|tx| {
        for (a_idx, (author, titles)) in AUTHORS.iter().enumerate() {
            for (t_idx, title) in titles.iter().enumerate() {
                let book_id = catalog::create_book(tx, title, &[author.to_string()])?;
                book_count += 1;

                // Pick primary narrator
                let narrator1 = NARRATORS[(a_idx * 7 + t_idx) % NARRATORS.len()];
                let var1 = catalog::ensure_variant(tx, book_id, &[narrator1.to_string()])?;

                // Create primary copy on Internal or External
                let vol_id = if (a_idx + t_idx) % 3 == 0 {
                    v_external
                } else {
                    v_internal
                };
                let is_m4b = (a_idx + t_idx) % 2 == 0;
                let rel_path = format!("{author}/{title} [{narrator1}]");

                let duration_ms = (14_400 + ((a_idx * 31 + t_idx * 17) % 36_000)) as u64 * 1000;
                let bitrate = if is_m4b { 64 } else { 128 };
                let file_size = (duration_ms / 1000) * (bitrate as u64 * 1000 / 8);

                let c_enum = CopyEnumeration {
                    volume_id: vol_id,
                    root_id: None,
                    rel_path: rel_path.clone(),
                    is_single_file: is_m4b,
                    file_count: if is_m4b { 1 } else { 12 },
                    audio_file_count: if is_m4b { 1 } else { 12 },
                    total_size: file_size + 250_000,
                    audio_size: file_size,
                    quick_signature: format!("sig_{a_idx}_{t_idx}_1"),
                    detection_confidence: 1.0,
                    evidence_json: "{}".into(),
                    scan_id: None,
                };
                let copy1 = catalog::insert_copy(tx, var1, book_id, &c_enum)?;
                copy_count += 1;

                let tech = CopyTech {
                    audio_file_count: if is_m4b { 1 } else { 12 },
                    total_size: file_size + 250_000,
                    audio_size: file_size,
                    total_duration_ms: duration_ms,
                    primary_codec: Some(if is_m4b { "aac".into() } else { "mp3".into() }),
                    container: Some(if is_m4b { "m4b".into() } else { "mp3".into() }),
                    avg_bitrate_kbps: Some(bitrate),
                    sample_rate: Some(44100),
                    channels: Some(1),
                    is_lossless: false,
                    has_cover: true,
                    has_fb2: false,
                    has_chapters: is_m4b,
                    probe_errors: 0,
                };
                catalog::update_copy_tech(tx, copy1, &tech, 4, Some(narrator1))?;
                catalog::set_copy_quality(tx, copy1, if is_m4b { 88.0 } else { 76.0 })?;

                // Insert media files
                if is_m4b {
                    let m_enum = MediaEnumeration {
                        rel_path: format!("{rel_path}/{title}.m4b"),
                        file_name: format!("{title}.m4b"),
                        ext: "m4b".into(),
                        kind: "audio".into(),
                        size: file_size,
                        mtime: now_ts() - 86400 * 30,
                        sort_index: 0,
                    };
                    let (media_id, _) = catalog::upsert_media(tx, copy1, vol_id, &m_enum, None)?;
                    let probe = ProbeUpdate {
                        container: Some("m4b".into()),
                        codec: Some("aac".into()),
                        duration_ms: Some(duration_ms),
                        bitrate_kbps: Some(64),
                        channels: Some(1),
                        sample_rate: Some(44100),
                        title: Some(title.to_string()),
                        artist: Some(author.to_string()),
                        album: Some(title.to_string()),
                        narrator: Some(narrator1.to_string()),
                        year: Some(2018),
                        genre: Some("Audiobook".into()),
                        language: Some("rus".into()),
                        has_embedded_cover: true,
                        chapter_count: 14,
                        ..Default::default()
                    };
                    catalog::update_media_probe(tx, media_id, &probe)?;
                } else {
                    for part in 1..=12 {
                        let m_enum = MediaEnumeration {
                            rel_path: format!("{rel_path}/{part:02}.mp3"),
                            file_name: format!("{part:02}.mp3"),
                            ext: "mp3".into(),
                            kind: "audio".into(),
                            size: file_size / 12,
                            mtime: now_ts() - 86400 * 30,
                            sort_index: part as i64,
                        };
                        let (media_id, _) =
                            catalog::upsert_media(tx, copy1, vol_id, &m_enum, None)?;
                        let probe = ProbeUpdate {
                            container: Some("mp3".into()),
                            codec: Some("mp3".into()),
                            duration_ms: Some(duration_ms / 12),
                            bitrate_kbps: Some(128),
                            channels: Some(2),
                            sample_rate: Some(44100),
                            track_no: Some(part),
                            title: Some(format!("Часть {part}")),
                            artist: Some(author.to_string()),
                            album: Some(title.to_string()),
                            narrator: Some(narrator1.to_string()),
                            year: Some(2015),
                            genre: Some("Audiobook".into()),
                            language: Some("rus".into()),
                            has_embedded_cover: part == 1,
                            chapter_count: 0,
                            ..Default::default()
                        };
                        catalog::update_media_probe(tx, media_id, &probe)?;
                    }
                }

                // Add duplicate copy / variant for selected books to test duplicate UI
                if (a_idx + t_idx) % 4 == 0 {
                    let narrator2 = NARRATORS[(a_idx * 7 + t_idx + 1) % NARRATORS.len()];
                    let var2 = catalog::ensure_variant(tx, book_id, &[narrator2.to_string()])?;
                    let rel_path2 = format!("Library/{author}/{title} (читает {narrator2})");
                    let c_enum2 = CopyEnumeration {
                        volume_id: v_archive,
                        root_id: None,
                        rel_path: rel_path2.clone(),
                        is_single_file: true,
                        file_count: 1,
                        audio_file_count: 1,
                        total_size: file_size * 9 / 10,
                        audio_size: file_size * 9 / 10,
                        quick_signature: format!("sig_{a_idx}_{t_idx}_2"),
                        detection_confidence: 1.0,
                        evidence_json: "{}".into(),
                        scan_id: None,
                    };
                    let copy2 = catalog::insert_copy(tx, var2, book_id, &c_enum2)?;
                    copy_count += 1;
                    let tech2 = CopyTech {
                        audio_file_count: 1,
                        total_size: file_size * 9 / 10,
                        audio_size: file_size * 9 / 10,
                        total_duration_ms: duration_ms * 98 / 100,
                        primary_codec: Some("aac".into()),
                        container: Some("m4b".into()),
                        avg_bitrate_kbps: Some(64),
                        sample_rate: Some(44100),
                        channels: Some(1),
                        is_lossless: false,
                        has_cover: true,
                        has_fb2: false,
                        has_chapters: true,
                        probe_errors: 0,
                    };
                    catalog::update_copy_tech(tx, copy2, &tech2, 4, Some(narrator2))?;
                    catalog::set_copy_quality(tx, copy2, 85.0)?;

                    // Record duplicate candidate
                    let asm = Assessment {
                        score: 0.92,
                        verdict: DuplicateVerdict::DifferentNarrator,
                        evidence: vec![Evidence {
                            code: "same_title_diff_narrator".into(),
                            text: format!(
                                "Same book by {author}, narrator {narrator1} vs {narrator2}"
                            ),
                            weight: 0.92,
                        }],
                        acoustic_checked: false,
                    };
                    duplicates::upsert_candidate(tx, copy1, copy2, &asm, Some(copy1))?;
                }

                // Add optimization candidate for high-bitrate MP3
                if !is_m4b && (a_idx + t_idx) % 3 == 1 {
                    let rec = Recommendation {
                        kind: ConversionKind::LossyToLossy,
                        target: TargetFormat::M4bAac,
                        current_codec: "MP3".into(),
                        current_bitrate_kbps: Some(128),
                        target_bitrate_kbps: Some(64),
                        current_size: file_size,
                        estimated_size: file_size / 2,
                        reencode: true,
                        preserves_chapters: true,
                        notes: "Merge 12 MP3 files into a single M4B audiobook with AAC audio"
                            .into(),
                    };
                    optimize::replace_for_copy(tx, copy1, &[rec])?;
                }

                // Add metadata issue for selected copies
                if (a_idx + t_idx) % 7 == 3 {
                    issues::replace_for_book(
                        tx,
                        book_id,
                        &[NewIssue {
                            copy_id: Some(copy1),
                            kind: IssueKind::ReleaseGarbage,
                            field: Some("title".into()),
                            message: "Title contains technical tag '[MP3, 128 kbps]'".into(),
                            current_value: Some(format!("{title} [MP3, 128 kbps]")),
                            proposed_value: Some(title.to_string()),
                        }],
                    )?;
                }

                catalog::refresh_book(tx, book_id)?;
            }
        }
        Ok(())
    })?;

    println!("Seeded {book_count} books, {copy_count} copies successfully into catalog.");
    Ok(())
}
