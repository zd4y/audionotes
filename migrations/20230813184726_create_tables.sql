create table users (
        id serial primary key,
        email varchar(255) unique not null,
        password varchar(255)
);

create table audios (
        id serial primary key,
        transcription text,
        created_at timestamptz not null default now(),
        user_id int not null,

        foreign key (user_id) references users (id)
);

create table password_reset_tokens (
        user_id int not null,
        token varchar(255) not null unique,
        expires_at timestamptz not null default now() + interval '5 minutes',

        primary key (user_id, token),
        foreign key (user_id) references users (id)
);

create table tags (
        user_id int not null,
        name varchar(25) not null,
        color varchar(7) not null default '#ffffff',

        primary key (user_id, name),
        foreign key (user_id) references users (id)
);

create table audio_tags (
        tag_user_id int not null,
        tag_name varchar(25) not null,
        audio_id int not null,

        primary key (tag_name, audio_id),
        foreign key (tag_user_id, tag_name) references tags (user_id, name),
        foreign key (audio_id) references audios (id)
)
