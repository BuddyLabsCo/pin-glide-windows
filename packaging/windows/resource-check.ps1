param(
    [string] $Executable,
    [ValidateSet('Receiver', 'Setup', 'Uninstaller')]
    [string] $Kind
)

$ErrorActionPreference = 'Stop'
if (!('BrandResources' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;

public static class BrandResources {
    delegate bool ResourceName(IntPtr module, IntPtr type, IntPtr name, IntPtr parameter);
    delegate bool ResourceLanguage(IntPtr module, IntPtr type, IntPtr name, ushort language, IntPtr parameter);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr LoadLibraryEx(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll")] static extern bool FreeLibrary(IntPtr module);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    static extern bool EnumResourceNames(IntPtr module, IntPtr type, ResourceName callback, IntPtr parameter);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    static extern bool EnumResourceLanguages(IntPtr module, IntPtr type, IntPtr name, ResourceLanguage callback, IntPtr parameter);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    static extern IntPtr FindResourceEx(IntPtr module, IntPtr type, IntPtr name, ushort language);
    [DllImport("kernel32.dll")] static extern IntPtr LoadResource(IntPtr module, IntPtr resource);
    [DllImport("kernel32.dll")] static extern IntPtr LockResource(IntPtr resource);
    [DllImport("kernel32.dll")] static extern int SizeofResource(IntPtr module, IntPtr resource);
    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    static extern int SHGetLocalizedName(string path, StringBuilder module, int length, out int id);
    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    public static extern int SHRemoveLocalizedName(string path);
    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    public static extern int SHSetLocalizedName(string path, string module, int id);
    [DllImport("shlwapi.dll", CharSet = CharSet.Unicode)]
    static extern int SHLoadIndirectString(string source, StringBuilder text, int length, IntPtr reserved);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool SetThreadPreferredUILanguages(uint flags, string languages, out uint count);
    [StructLayout(LayoutKind.Sequential)]
    struct PropertyKey { public Guid format; public int id; }
    [StructLayout(LayoutKind.Explicit, Size = 24)]
    struct PropertyValue {
        [FieldOffset(0)] public ushort type;
        [FieldOffset(8)] public IntPtr text;
    }
    [ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"),
        InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface PropertyStore {
        [PreserveSig] int GetCount(out int count);
        [PreserveSig] int GetAt(int index, out PropertyKey key);
        [PreserveSig] int GetValue(ref PropertyKey key, out PropertyValue value);
    }
    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    static extern int SHGetPropertyStoreFromParsingName(string path, IntPtr bind, int flags, ref Guid iid, out PropertyStore store);
    [DllImport("ole32.dll")] static extern int PropVariantClear(ref PropertyValue value);

    static IntPtr Open(string path) {
        IntPtr module = LoadLibraryEx(Path.GetFullPath(path), IntPtr.Zero, 0x22);
        if (module == IntPtr.Zero) throw new System.ComponentModel.Win32Exception();
        return module;
    }
    static int[] Names(IntPtr module, int type) {
        var names = new List<int>();
        EnumResourceNames(module, (IntPtr)type, (m, t, n, p) => {
            if (n.ToInt64() <= 65535) names.Add(n.ToInt32());
            return true;
        }, IntPtr.Zero);
        return names.ToArray();
    }
    static ushort[] Languages(IntPtr module, int type, int name) {
        var languages = new List<ushort>();
        EnumResourceLanguages(module, (IntPtr)type, (IntPtr)name, (m, t, n, l, p) => {
            languages.Add(l); return true;
        }, IntPtr.Zero);
        return languages.ToArray();
    }
    static byte[] Read(IntPtr module, int type, int name, ushort language) {
        IntPtr resource = FindResourceEx(module, (IntPtr)type, (IntPtr)name, language);
        if (resource == IntPtr.Zero) throw new Exception("Missing PE resource " + type + "/" + name + "/" + language);
        var bytes = new byte[SizeofResource(module, resource)];
        Marshal.Copy(LockResource(LoadResource(module, resource)), bytes, 0, bytes.Length);
        return bytes;
    }
    static int U16(byte[] bytes, int offset) { return BitConverter.ToUInt16(bytes, offset); }
    static int I32(byte[] bytes, int offset) { return BitConverter.ToInt32(bytes, offset); }
    static int Align(int offset) { return (offset + 3) & ~3; }

    public static void AssertIcon(string path, string icon, int requiredGroup) {
        var expected = File.ReadAllBytes(icon);
        int count = U16(expected, 4);
        if (count < 3) throw new Exception("Brand ICO must contain multiple sizes");
        IntPtr module = Open(path);
        try {
            foreach (int name in Names(module, 14)) {
                if (requiredGroup != 0 && name != requiredGroup) continue;
                foreach (ushort language in Languages(module, 14, name)) {
                    byte[] group = Read(module, 14, name, language);
                    if (U16(group, 4) != count) continue;
                    var images = new List<byte[]>();
                    for (int i = 0; i < count; i++) {
                        int id = U16(group, 6 + 14 * i + 12);
                        images.Add(Read(module, 3, id, language));
                    }
                    bool matches = true;
                    for (int i = 0; i < count; i++) {
                        int directory = 6 + 16 * i;
                        byte[] image = expected.Skip(I32(expected, directory + 12))
                            .Take(I32(expected, directory + 8)).ToArray();
                        if (!images.Any(actual => actual.SequenceEqual(image))) matches = false;
                    }
                    if (matches) return;
                }
            }
            throw new Exception("Missing multi-size brand icon in " + path);
        } finally { FreeLibrary(module); }
    }

    public static Dictionary<ushort, string> StringResources(string path, int id) {
        IntPtr module = Open(path);
        try {
            int block = id / 16 + 1;
            var strings = new Dictionary<ushort, string>();
            foreach (ushort language in Languages(module, 6, block)) {
                byte[] bytes = Read(module, 6, block, language);
                int offset = 0;
                for (int i = 0; i < id % 16; i++) offset += 2 + U16(bytes, offset) * 2;
                strings[language] = Encoding.Unicode.GetString(bytes, offset + 2, U16(bytes, offset) * 2);
            }
            return strings;
        } finally { FreeLibrary(module); }
    }

    static void VersionValues(byte[] bytes, int offset, string parent, Dictionary<string, string> values) {
        int length = U16(bytes, offset);
        int valueLength = U16(bytes, offset + 2);
        int type = U16(bytes, offset + 4);
        int keyEnd = offset + 6;
        while (U16(bytes, keyEnd) != 0) keyEnd += 2;
        string key = parent + "/" + Encoding.Unicode.GetString(bytes, offset + 6, keyEnd - offset - 6);
        int value = Align(keyEnd + 2);
        if (type == 1 && valueLength > 0)
            values[key] = Encoding.Unicode.GetString(bytes, value, valueLength * 2).TrimEnd('\0');
        for (int child = Align(value + valueLength * (type == 1 ? 2 : 1));
             child + 6 <= offset + length && U16(bytes, child) > 0;
             child = Align(child + U16(bytes, child)))
            VersionValues(bytes, child, key, values);
    }

    public static Dictionary<string, string> Versions(string path) {
        IntPtr module = Open(path);
        try {
            var values = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            foreach (int name in Names(module, 16))
                foreach (ushort language in Languages(module, 16, name))
                    VersionValues(Read(module, 16, name, language), 0, "", values);
            return values;
        } finally { FreeLibrary(module); }
    }

    public static string ShortcutResource(string path) {
        var module = new StringBuilder(32768);
        int id;
        return SHGetLocalizedName(path, module, module.Capacity, out id) == 0
            ? module.ToString() + "," + id : null;
    }

    public static string ShortcutIdentity(string path) {
        Guid iid = typeof(PropertyStore).GUID;
        PropertyStore store;
        Marshal.ThrowExceptionForHR(SHGetPropertyStoreFromParsingName(path, IntPtr.Zero, 0, ref iid, out store));
        try {
            var key = new PropertyKey { format = new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"), id = 5 };
            PropertyValue value;
            Marshal.ThrowExceptionForHR(store.GetValue(ref key, out value));
            try {
                return value.type == 31 ? Marshal.PtrToStringUni(value.text) : null;
            } finally { PropVariantClear(ref value); }
        } finally { Marshal.ReleaseComObject(store); }
    }

    public static string ResolveName(string reference, string language) {
        string result = null;
        Exception failure = null;
        var thread = new System.Threading.Thread(() => {
            try {
                uint count;
                if (!SetThreadPreferredUILanguages(4, language + "\0\0", out count))
                    throw new System.ComponentModel.Win32Exception();
                if (count != 1)
                    throw new Exception("Could not establish the requested single UI language");
                var text = new StringBuilder(256);
                Marshal.ThrowExceptionForHR(SHLoadIndirectString(reference, text, text.Capacity, IntPtr.Zero));
                result = text.ToString();
            } catch (Exception error) { failure = error; }
        });
        thread.Start();
        thread.Join();
        if (failure != null) throw failure;
        return result;
    }
}
'@
}

function Assert-BrandResources([string] $path, [string] $kind) {
    $path = (Resolve-Path -LiteralPath $path).Path
    $icon = Join-Path $PSScriptRoot '../../crates/windows-app/ui/assets/app-icon.ico'
    $group = if ($kind -eq 'Receiver') { 1 } else { 0 }
    [BrandResources]::AssertIcon($path, $icon, $group)
    $english = 'Pin Glide'
    $chinese = [string][char]0x9e4a + [char]0x4fe1
    $receiverLanguages = @(1..0x03ff) + @(1033, 1028, 2052, 3076, 4100, 5124, 0x7804, 0x7c04)
    if ($kind -eq 'Receiver') {
        $strings = [BrandResources]::StringResources($path, 101)
        foreach ($language in $receiverLanguages) {
            $expected = if (($language -band 0x03ff) -eq 4) { $chinese } else { $english }
            if ($strings[[uint16]$language] -cne $expected) {
                throw "Wrong Receiver string resource for language $language"
            }
        }
        foreach ($language in '0409', '0411', '0407', '0412', '0c0c', '0c0a', '0401', '0419', '0809') {
            if ([BrandResources]::ResolveName("@$path,-101", $language) -cne $english) {
                throw "Native resource lookup did not select English for $language in $path"
            }
        }
        foreach ($language in '0004', '0404', '0804', '0c04', '1004', '1404', '7804', '7c04') {
            if ([BrandResources]::ResolveName("@$path,-101", $language) -cne $chinese) {
                throw "Native resource lookup did not select Chinese for $language in $path"
            }
        }
    }
    $versions = [BrandResources]::Versions($path)
    if ($kind -eq 'Receiver') {
        foreach ($language in $receiverLanguages) {
            $expected = if (($language -band 0x03ff) -eq 4) { $chinese } else { $english }
            $prefix = '/VS_VERSION_INFO/StringFileInfo/{0:x4}04b0' -f $language
            if ($versions["$prefix/ProductName"] -cne $expected -or
                $versions["$prefix/FileDescription"] -cne $expected) {
                throw "Missing localized Receiver version metadata for $language in $path"
            }
        }
        return
    }
    $languages = '0409', '0804'
    foreach ($language in $languages) {
        $prefix = "/VS_VERSION_INFO/StringFileInfo/$language"
        $expected = if ($language -eq '0409') { $english } else { $chinese }
        $name = $versions.GetEnumerator() | Where-Object { $_.Key -like "$prefix*/ProductName" }
        $description = $versions.GetEnumerator() | Where-Object { $_.Key -like "$prefix*/FileDescription" }
        if ($name.Value -cne $expected -or !$description.Value.StartsWith($expected)) {
            throw "Missing localized $kind version metadata for $language in $path"
        }
    }
}

if ($Executable) {
    Assert-BrandResources $Executable $Kind
}
