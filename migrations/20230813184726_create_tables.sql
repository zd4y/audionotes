create table audios (
        id serial primary key,
        transcription text,
        file_sha256_hex VARCHAR (64) not null,
        created_at timestamp not null
);

create table users (
        id serial primary key,
        username VARCHAR (50) unique not null,
        password VARCHAR (255)
);
