create table audios (
        id serial primary key,
        transcription text,
        file_sha256_hex varchar (64) not null,
        created_at timestamp not null
);

create table users (
        id serial primary key,
        username varchar (50) unique not null,
        password varchar (255),
        user_id integer not null
);
