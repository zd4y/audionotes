create table failed_audio_transcriptions (
    id serial primary key,
    audio_id int not null,
    retries int not null default 0,
    language char(2) not null,
    created_at timestamptz not null default now(),
    last_retry_at timestamptz,

    foreign key (audio_id) references audios (id) on delete cascade
)
