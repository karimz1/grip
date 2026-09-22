/*
 * lock-fixture.c
 *
 * Cross-platform file-lock fixture for testing tools such as oflh.
 *
 * Windows:
 *   - Uses CreateFileA() sharing modes
 *   - Uses LockFileEx() for byte-range locks
 *
 * macOS/Linux:
 *   - Uses open()
 *   - Uses fcntl(F_SETLK) POSIX byte-range locks
 *
 * Build:
 *
 *   macOS/Linux:
 *     cc -Wall -Wextra -O2 lock-fixture.c -o lock-fixture
 *
 *   Windows (MSVC):
 *     cl /W4 /O2 lock-fixture.c
 *
 *   Windows (MinGW):
 *     gcc -Wall -Wextra -O2 lock-fixture.c -o lock-fixture.exe
 *
 * Usage:
 *
 *   lock-fixture <mode> <file> [start] [length]
 *
 * Modes:
 *
 *   open
 *       Open the file and keep it open without a byte-range lock.
 *
 *   read
 *       Acquire a shared/read lock over the whole file.
 *
 *   write
 *       Acquire an exclusive/write lock over the whole file.
 *
 *   range
 *       Acquire an exclusive/write lock over a byte range.
 *
 *       Example:
 *         lock-fixture range test.dat 100 500
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>

#ifdef _WIN32

#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#else

#include <unistd.h>
#include <fcntl.h>
#include <sys/types.h>

#endif


static void usage(const char *program)
{
    fprintf(stderr,
        "Usage:\n"
        "  %s open  <file>\n"
        "  %s read  <file>\n"
        "  %s write <file>\n"
        "  %s range <file> <start> <length>\n",
        program,
        program,
        program,
        program);
}


static long long parse_number(const char *value, const char *name)
{
    char *end = NULL;

    errno = 0;

    long long result = strtoll(value, &end, 10);

    if (errno != 0 || end == value || *end != '\0' || result < 0)
    {
        fprintf(stderr, "Invalid %s: %s\n", name, value);
        exit(2);
    }

    return result;
}


#ifdef _WIN32

/* ============================================================
 * Windows
 * ============================================================ */

static void print_windows_error(const char *operation)
{
    DWORD error = GetLastError();

    char *message = NULL;

    FormatMessageA(
        FORMAT_MESSAGE_ALLOCATE_BUFFER |
        FORMAT_MESSAGE_FROM_SYSTEM |
        FORMAT_MESSAGE_IGNORE_INSERTS,
        NULL,
        error,
        0,
        (LPSTR)&message,
        0,
        NULL);

    fprintf(stderr,
        "%s failed.\n"
        "Win32 error: %lu\n"
        "Message: %s\n",
        operation,
        (unsigned long)error,
        message != NULL ? message : "(unknown)");

    if (message != NULL)
        LocalFree(message);
}


static OVERLAPPED make_overlapped(unsigned long long start)
{
    OVERLAPPED ov;

    memset(&ov, 0, sizeof(ov));

    ov.Offset =
        (DWORD)(start & 0xffffffffULL);

    ov.OffsetHigh =
        (DWORD)((start >> 32) & 0xffffffffULL);

    return ov;
}


static int run_windows(
    const char *mode,
    const char *path,
    unsigned long long start,
    unsigned long long length)
{
    DWORD shareMode;

    /*
     * "open" demonstrates a normal open handle.
     *
     * Other modes deliberately deny sharing so tools can observe
     * a strong Windows sharing restriction as well as LockFileEx.
     */
    if (strcmp(mode, "open") == 0)
    {
        shareMode =
            FILE_SHARE_READ |
            FILE_SHARE_WRITE |
            FILE_SHARE_DELETE;
    }
    else
    {
        shareMode = 0;
    }

    HANDLE file = CreateFileA(
        path,
        GENERIC_READ | GENERIC_WRITE,
        shareMode,
        NULL,
        OPEN_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        NULL);

    if (file == INVALID_HANDLE_VALUE)
    {
        print_windows_error("CreateFile");
        return 1;
    }

    printf("PID:      %lu\n", (unsigned long)GetCurrentProcessId());
    printf("File:     %s\n", path);
    printf("Handle:   %p\n", file);
    printf("Mode:     %s\n", mode);

    if (strcmp(mode, "open") == 0)
    {
        printf("Sharing:  read/write/delete allowed\n");
        printf("Lock:     none\n");
    }
    else
    {
        DWORD flags = 0;

        if (strcmp(mode, "write") == 0 ||
            strcmp(mode, "range") == 0)
        {
            flags |= LOCKFILE_EXCLUSIVE_LOCK;
        }

        /*
         * Windows requires a finite range.
         *
         * For whole-file locks use the maximum practical range.
         */
        unsigned long long lockStart = start;
        unsigned long long lockLength = length;

        if (strcmp(mode, "read") == 0 ||
            strcmp(mode, "write") == 0)
        {
            lockStart = 0;
            lockLength = 0xffffffffffffffffULL;
        }

        OVERLAPPED ov = make_overlapped(lockStart);

        DWORD lengthLow =
            (DWORD)(lockLength & 0xffffffffULL);

        DWORD lengthHigh =
            (DWORD)((lockLength >> 32) & 0xffffffffULL);

        if (!LockFileEx(
                file,
                flags | LOCKFILE_FAIL_IMMEDIATELY,
                0,
                lengthLow,
                lengthHigh,
                &ov))
        {
            print_windows_error("LockFileEx");
            CloseHandle(file);
            return 1;
        }

        printf(
            "Lock:     %s\n",
            strcmp(mode, "read") == 0
                ? "shared/read"
                : "exclusive/write");

        printf(
            "Range:    %llu .. ",
            lockStart);

        if (strcmp(mode, "read") == 0 ||
            strcmp(mode, "write") == 0)
        {
            printf("EOF\n");
        }
        else
        {
            printf(
                "%llu (%llu bytes)\n",
                lockStart + lockLength,
                lockLength);
        }
    }

    printf("\n");
    printf("LOCK FIXTURE READY\n");
    printf("Press ENTER to release/close...\n");

    fflush(stdout);
    getchar();

    CloseHandle(file);

    printf("Released.\n");

    return 0;
}

#else

/* ============================================================
 * macOS / Linux
 * ============================================================ */

static int run_posix(
    const char *mode,
    const char *path,
    long long start,
    long long length)
{
    int fd = open(
        path,
        O_RDWR | O_CREAT,
        0644);

    if (fd == -1)
    {
        fprintf(stderr,
            "open failed: %s\n",
            strerror(errno));

        return 1;
    }

    printf("PID:      %ld\n", (long)getpid());
    printf("File:     %s\n", path);
    printf("FD:       %d\n", fd);
    printf("Mode:     %s\n", mode);

    if (strcmp(mode, "open") == 0)
    {
        printf("Lock:     none\n");
    }
    else
    {
        struct flock lock;

        memset(&lock, 0, sizeof(lock));

        lock.l_whence = SEEK_SET;

        if (strcmp(mode, "read") == 0)
        {
            lock.l_type = F_RDLCK;
            lock.l_start = 0;

            /*
             * POSIX:
             * l_len == 0 means through EOF.
             */
            lock.l_len = 0;
        }
        else if (strcmp(mode, "write") == 0)
        {
            lock.l_type = F_WRLCK;
            lock.l_start = 0;
            lock.l_len = 0;
        }
        else
        {
            lock.l_type = F_WRLCK;
            lock.l_start = (off_t)start;
            lock.l_len = (off_t)length;
        }

        if (fcntl(fd, F_SETLK, &lock) == -1)
        {
            fprintf(stderr,
                "fcntl(F_SETLK) failed: %s\n",
                strerror(errno));

            close(fd);

            return 1;
        }

        printf(
            "Lock:     %s\n",
            lock.l_type == F_RDLCK
                ? "POSIX F_RDLCK"
                : "POSIX F_WRLCK");

        printf(
            "Range:    %lld .. ",
            (long long)lock.l_start);

        if (lock.l_len == 0)
        {
            printf("EOF\n");
        }
        else
        {
            printf(
                "%lld (%lld bytes)\n",
                (long long)(lock.l_start + lock.l_len),
                (long long)lock.l_len);
        }
    }

    printf("\n");
    printf("LOCK FIXTURE READY\n");
    printf("Press ENTER to release/close...\n");

    fflush(stdout);
    getchar();

    /*
     * close() releases POSIX record locks held by this process
     * for this file.
     */
    close(fd);

    printf("Released.\n");

    return 0;
}

#endif


int main(int argc, char **argv)
{
    if (argc < 3)
    {
        usage(argv[0]);
        return 2;
    }

    const char *mode = argv[1];
    const char *path = argv[2];

    int validMode =
        strcmp(mode, "open") == 0 ||
        strcmp(mode, "read") == 0 ||
        strcmp(mode, "write") == 0 ||
        strcmp(mode, "range") == 0;

    if (!validMode)
    {
        fprintf(stderr,
            "Unknown mode: %s\n\n",
            mode);

        usage(argv[0]);
        return 2;
    }

    long long start = 0;
    long long length = 0;

    if (strcmp(mode, "range") == 0)
    {
        if (argc != 5)
        {
            fprintf(stderr,
                "range requires <start> <length>\n\n");

            usage(argv[0]);
            return 2;
        }

        start = parse_number(argv[3], "start");
        length = parse_number(argv[4], "length");

        if (length == 0)
        {
            fprintf(stderr,
                "Range length must be greater than zero.\n");

            return 2;
        }
    }

#ifdef _WIN32

    return run_windows(
        mode,
        path,
        (unsigned long long)start,
        (unsigned long long)length);

#else

    return run_posix(
        mode,
        path,
        start,
        length);

#endif
}