create table users (
        id serial primary key,
        email varchar (254) unique not null,
        username varchar (50) unique not null,
        password varchar (255)
);

create table audios (
        id serial primary key,
        transcription text,
        created_at timestamptz not null default now(),
        user_id int not null,

        foreign key (user_id)
                references users (id)
);
