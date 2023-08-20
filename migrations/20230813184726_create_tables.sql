create table users (
        id serial primary key,
        email varchar(255) unique not null,
        password varchar(255)
);

create table audios (
        id serial primary key,
        transcription text,
        length int not null,
        created_at timestamptz not null default now(),
        user_id int not null,

        foreign key (user_id)
                references users (id)
);

create table password_reset_tokens (
        user_id int not null,
        token varchar(255) not null unique,
        expires_at timestamptz not null default now() + interval '5 minutes',

        primary key (user_id, token),

        foreign key (user_id)
                references users (id)
);
