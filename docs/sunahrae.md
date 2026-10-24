# 세벌식 최종 순아래 관리 안내

[youknowone의 세벌식 최종 순아래](https://text.youknowone.org/post/106848470561/3final-noshift)를 위한 kime 수정본이다. LICENSE와 NOTICE.md를 따른다.

## 지원 범위와 제약

- 된소리: ㄱ+ㅇ=ㄲ, ㅈ+ㄱ=ㅉ, ㅂ+ㅈ=ㅃ, ㄷ+ㅁ=ㄸ, ㅅ+ㅎ=ㅆ. 순서 무관
- 추가 모음: ㅗ+ㅓ=ㅝ, ㅗ+ㅔ=ㅞ, ㅗ+ㅡ=ㅒ, ㅗ+ㅕ=ㅖ, ㅗ+ㅗ=ㅢ, ㅜ+ㅏ=ㅘ, ㅜ+ㅐ=ㅙ
- 초성이 있으면 `[` 뒤의 키를 겹받침으로 해석. Shift 슬롯과 분리
- 초성이 없으면 `/`와 `[`는 리터럴 출력
- 순아래 addon은 `sebeolsik-sunahrae`에서 활성화
- 미구현: 후보 키 뒤의 `[` 소급 처리, 단독 `[` 위치 규칙, 조합용 ㅗ·ㅜ 단독 입력의 ㅚ·ㅟ 변환

## 원본 변경 통합

이 저장소는 `Riey/kime`을 기반으로 한 Fork이며, `sunahrae` 브랜치에서 순아래 입력기 관련 수정사항을 관리한다.

- `develop`: 원본 프로젝트의 변경사항을 추적하는 브랜치
- `sunahrae`: 순아래 수정사항을 포함한 빌드 대상 브랜치

Git 원격 저장소의 이름(`origin`, `upstream` 등)은 클론 환경에 따라 달라질 수 있으므로 `git remote -v`로 확인한다.

원본 프로젝트의 변경사항을 동기화하더라도 `sunahrae` 브랜치와 이미 설치된 입력기는 자동으로 갱신되지 않는다. 변경사항을 해당 브랜치에 반영한 뒤 다시 빌드하고 설치해야 한다.

아래 명령은 fish 기준이다. 미커밋 변경을 보존한 뒤 working tree가 깨끗할 때 진행한다.

```fish
cd ~/kime-sunahrae-patch/kime-src
git status --short --branch
git remote -v
git fetch origin
git fetch fork
git switch sunahrae
git branch backup/sunahrae-(date +%Y%m%d-%H%M%S)
git merge origin/develop
```

충돌 시 해결 후 검증하거나 `git merge --abort`로 취소한다. 공유 이력을 재작성하지 않는다. v3.2.0 태그는 최초 패치 기반 09ed7a2보다 오래되어 재적용 대상으로 사용하지 않는다.

## 빌드와 테스트

Debian 13, rustup stable, Meson, Ninja, C/C++ 컴파일러와 GTK3·GTK4·Qt5·Qt6 개발 패키지가 필요하다. 기존 설치가 있으면 재설치하지 않는다.

```fish
fish_add_path --path ~/.cargo/bin
cargo --version
cargo test --locked -p kime-engine-backend-hangul
set multiarch (dpkg-architecture -qDEB_HOST_MULTIARCH)
meson setup build --prefix=/usr --libdir=lib/$multiarch -Dgtk3=enabled -Dgtk4=enabled -Dqt5=enabled -Dqt6=enabled
ninja -C build
```

build가 이미 구성돼 있으면 setup 대신 `meson setup --reconfigure build`를 사용한다. 원본 9601185 이후 Rust 산출물은 `build/target/release/`에 생성된다.

## 패키징과 설치

기존 패키지를 보존하도록 별도 출력 폴더를 사용한다.

```fish
set package_dir ~/kime-sunahrae-patch/out/(date +%Y%m%d-%H%M%S)
mkdir -p $package_dir
bash packaging/build-deb.sh build $package_dir
set package_file $package_dir/kime_(string trim < VERSION)_(dpkg --print-architecture).deb
dpkg-deb --info $package_file
sudo dpkg -i $package_file
```

sudo 비밀번호는 터미널에 직접 입력한다. 설치 후 작업을 저장하고 로그아웃·재로그인하여 입력기와 앱 모듈을 다시 로드한다. 기존 설정을 유지하면서 `~/.config/kime/config.yaml`의 `engine.hangul.layout`을 `sebeolsik-sunahrae`로 선택한다.

## 입력 검증

- 된소리 5종과 추가 모음 7종
- ㄱ + ㅏ + `[` + ㄱ → 값
- 조합 중이 아닐 때 `/`, `[` 리터럴
- Shift+숫자 → `!@#$%^&*()_+`
- GTK·Qt 앱의 조합과 한영 전환

단위 테스트와 실제 데스크톱 입력 검증은 구분한다.

## 백업

수정 파일만 스테이징·커밋한 뒤 일반 push를 사용한다.

```fish
git push fork sunahrae
git diff --binary origin/develop...sunahrae -- src/engine/backends/hangul > ../sunahrae-engine.patch
```

패치는 `git apply`용 diff이다. 적용 대상에서 먼저 `git apply --check`로 검사한다.
