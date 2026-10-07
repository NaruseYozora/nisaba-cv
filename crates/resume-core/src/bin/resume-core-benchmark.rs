use resume_core::{Store, model::*};
use serde_json::json;
use std::{path::PathBuf, time::Instant};
fn timing(samples: &mut Vec<f64>, action: impl FnOnce()) {
    let start = Instant::now();
    action();
    samples.push(start.elapsed().as_secs_f64() * 1000.0);
}
fn stats(mut values: Vec<f64>) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    json!({"samples":values.len(),"medianMs":values[values.len()/2],"p95Ms":values[((values.len()-1) as f64*0.95).ceil() as usize],"maxMs":values.last()})
}
fn main() {
    let base = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("isolated benchmark output directory"),
    );
    std::fs::create_dir_all(&base).unwrap();
    let root = base.join(format!("benchmark-{}", resume_core::store::id()));
    let mut store = Store::open(&root).unwrap();
    let seed = Instant::now();
    let mut items = vec![];
    for n in 0..500 {
        items.push(store.save_item(None,None,ItemDraft{content:ItemContent::Work{company:format!("匿名经历{n:04}"),role:"平台工程师".into(),department:"研发".into(),location:"上海".into(),dates:DateRange{start:Some(PartialDate{year:2021,month:None}),end:None,ongoing:true}},tags:vec![format!("标签{}",n%10)],notes:"内部记录不导出".repeat(8),achievements:(0..4).map(|i|AchievementDraft{id:None,text:format!("成果{n:04}-{i}：建立稳定的任务处理与本地资料保存流程，接口延迟减少 30%。").repeat(3)}).collect()}).unwrap());
    }
    let profile = store.profile().unwrap();
    store
        .save_profile(
            ProfileDraft {
                name: "匿名性能测试".into(),
                summary: "维护个人资料库并针对岗位组织经历。".into(),
                ..profile.content
            },
            profile.revision,
        )
        .unwrap();
    let mut resumes = vec![];
    for n in 0..100 {
        let selection = Selection {
            custom_field_ids: vec![],
            sections: None,
            profile_fields: vec![ProfileField::Name, ProfileField::Summary],
            items: items
                .iter()
                .skip(n * 4)
                .take(10)
                .map(|i| SelectionItem {
                    item_id: i.id.clone(),
                    achievement_ids: i.achievements.iter().map(|a| a.id.clone()).collect(),
                    include_background: false,
                })
                .collect(),
        };
        resumes.push(
            store
                .create_resume(&format!("岗位简历{n:03}"), &selection, Style::default())
                .unwrap(),
        );
    }
    for n in 0..200 {
        store
            .record_export(
                &resumes[n % 100],
                include_bytes!("../../tests/fixtures/anonymous.pdf"),
            )
            .unwrap();
    }
    let seed_ms = seed.elapsed().as_secs_f64() * 1000.0;
    let mut searches = vec![];
    let mut saves = vec![];
    let mut resume_saves = vec![];
    for n in 0..60 {
        timing(&mut searches, || {
            assert!(
                !store
                    .items(RecordState::Active, &format!("成果{:04}", n * 7 % 500))
                    .unwrap()
                    .is_empty()
            );
        });
    }
    for (n, sample) in resumes.iter().enumerate().take(30) {
        let p = store.profile().unwrap();
        timing(&mut saves, || {
            store
                .save_profile(
                    ProfileDraft {
                        title: format!("保存样本{n}"),
                        ..p.content
                    },
                    p.revision,
                )
                .unwrap();
        });
        let r = store.resume(&sample.id).unwrap();
        timing(&mut resume_saves, || {
            store
                .save_resume(
                    &r.id,
                    r.revision,
                    ResumeDraft {
                        name: r.name,
                        company: "目标公司".into(),
                        role: format!("岗位{n}"),
                        document: r.document,
                    },
                )
                .unwrap();
        });
    }
    let start = Instant::now();
    let backup = base.join(format!(
        "performance-{}.rslbackup",
        resume_core::store::id()
    ));
    store.backup(&backup).unwrap();
    let backup_ms = start.elapsed().as_secs_f64() * 1000.0;
    let preview = store.inspect_backup(&backup).unwrap();
    let start = Instant::now();
    store
        .restore(
            &backup,
            &preview.package_sha256,
            resume_core::backup::RestoreFault::None,
        )
        .unwrap();
    let restore_ms = start.elapsed().as_secs_f64() * 1000.0;
    drop(store);
    let start = Instant::now();
    let store = Store::open(&root).unwrap();
    let reopen_ms = start.elapsed().as_secs_f64() * 1000.0;
    let counts = store.overview().unwrap().counts;
    let report = json!({"passed":true,"build":if cfg!(debug_assertions){"debug"}else{"release"},"root":root,"counts":counts,"seedMs":seed_ms,"search":stats(searches),"profileSave":stats(saves),"resumeSave":stats(resume_saves),"backupMs":backup_ms,"restoreMs":restore_ms,"reopenMs":reopen_ms,"databaseBytes":std::fs::metadata(root.join("library.sqlite3")).unwrap().len(),"backupBytes":std::fs::metadata(backup).unwrap().len(),"scope":"real SQLite core operations; UI rendering measured separately"});
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
