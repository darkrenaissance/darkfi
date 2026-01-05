//% IMPORTS

import android.view.ViewGroup;
import android.view.WindowInsets.Type;
import android.view.inputmethod.EditorInfo;
import android.text.InputType;
import android.util.Log;
import java.util.HashMap;

import videodecode.VideoDecoder;
import textinput.Settings;
import textinput.Listener;
import textinput.State;
import textinput.GameTextInput;

//% END

//% QUAD_SURFACE_ON_CREATE_INPUT_CONNECTION

MainActivity main = (MainActivity)getContext();
// Create InputConnection if it doesn't exist yet
if (main.inpcon == null) {
    EditorInfo editorInfo = new EditorInfo();
    editorInfo.inputType = InputType.TYPE_CLASS_TEXT |
                           InputType.TYPE_TEXT_FLAG_MULTI_LINE |
                           InputType.TYPE_TEXT_FLAG_AUTO_CORRECT;
    editorInfo.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN;

    main.inpcon = new textinput.InputConnection(
        getContext(),
        this,
        new Settings(editorInfo, true)
    );

    // Pass the InputConnection to native GameTextInput library
    main.setInputConnectionNative(main.inpcon);
}

// Set the listener to receive IME state changes
main.inpcon.setListener(new Listener() {
    @Override
    public void stateChanged(State newState, boolean dismissed) {
        // Called when the IME sends new text state
        // Forward to native code which triggers Rust callback
        Log.d("darkfi", "stateChanged: text=" + newState.toString());
        main.onTextInputEventNative(newState);
    }

    @Override
    public void onImeInsetsChanged(android.graphics.Insets insets) {
        // Called when IME insets change (e.g., keyboard height changes)
        // Optional: can be used for dynamic layout adjustment
    }

    @Override
    public void onSoftwareKeyboardVisibilityChanged(boolean visible) {
        // Called when keyboard is shown or hidden
    }

    @Override
    public void onEditorAction(int actionCode) {
        // Called when user presses action button (Done, Next, etc.)
        // Optional: handle specific editor actions
    }
});

// Copy EditorInfo from GameTextInput to configure IME
if (outAttrs != null) {
    GameTextInput.copyEditorInfo(
        main.inpcon.getEditorInfo(),
        outAttrs
    );
}

// Return the GameTextInput InputConnection to IME
if (true) return main.inpcon;

//% END

//% RESIZING_LAYOUT_BODY

native static void onApplyInsets(
    int sys_left, int sys_top, int sys_right, int sys_bottom,
    int ime_left, int ime_top, int ime_right, int ime_bottom
);

//% END

//% RESIZING_LAYOUT_ON_APPLY_WINDOW_INSETS

{
    Insets imeInsets = insets.getInsets(WindowInsets.Type.ime());
    Insets sysInsets = insets.getInsets(WindowInsets.Type.systemBars());

    // Screen: (1440, 3064)
    // IME height: 1056
    // Sys insets: (0, 152, 0, 135)

    onApplyInsets(
        sysInsets.left, sysInsets.top, sysInsets.right, sysInsets.bottom,
        imeInsets.left, imeInsets.top, imeInsets.right, imeInsets.bottom
    );
}
// Workaround for Java error due to remaining body.
// We handle the insets in our app directly.
if (true)
    return insets;

//% END

//% MAIN_ACTIVITY_BODY

// GameTextInput native bridge functions (following official Android pattern)
native void setInputConnectionNative(textinput.InputConnection c);
native void onTextInputEventNative(textinput.State softKeyboardEvent);

// GameTextInput InputConnection reference (public for QuadSurface access)
public textinput.InputConnection inpcon;

public String getAppDataPath() {
    return getApplicationContext().getDataDir().getAbsolutePath();
}
public String getExternalStoragePath() {
    return getApplicationContext().getExternalFilesDir(null).getAbsolutePath();
}

public int getKeyboardHeight() {
    WindowInsets windowInsets = view.getRootWindowInsets();
    if (windowInsets == null) {
        return 0;
    }
    Insets imeInsets = windowInsets.getInsets(WindowInsets.Type.ime());
    Insets navInsets = windowInsets.getInsets(WindowInsets.Type.navigationBars());
    return Math.max(0, imeInsets.bottom - navInsets.bottom);
}

public float getScreenDensity() {
    return getResources().getDisplayMetrics().density;
}

public boolean isImeVisible() {
    View decorView = getWindow().getDecorView();
    WindowInsets insets = decorView.getRootWindowInsets();
    Insets imeInsets = insets.getInsets(WindowInsets.Type.ime());
    if (imeInsets == null)
        return false;
    return insets.isVisible(Type.ime());
}

public VideoDecoder createVideoDecoder() {
    VideoDecoder decoder = new VideoDecoder();
    decoder.setContext(this);
    return decoder;
}

//% END

//% MAIN_ACTIVITY_ON_CREATE

// Start a foreground service so the app stays awake
Intent serviceIntent = new Intent(this, ForegroundService.class);
startForegroundService(serviceIntent);

//% END

