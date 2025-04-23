//% IMPORTS

import android.view.ViewGroup;
import android.text.Editable;
import android.text.Spannable;
import android.text.SpanWatcher;
import android.text.Spanned;
import android.text.TextWatcher;
import android.widget.EditText;
import android.widget.TextView;
import android.view.inputmethod.BaseInputConnection;
import java.util.HashMap;

import autosuggest.InvisibleInputView;
import autosuggest.CustomInputConnection;

//% END

//% MAIN_ACTIVITY_BODY

private ViewGroup rootView;

private HashMap<Integer, InvisibleInputView> editors;

native static void onInitEdit(int id);

public void createComposer(final int id) {
    Log.d("darkfi", "createComposer() -> " + id);

    final InvisibleInputView iv = new InvisibleInputView(this, id);
    editors.put(id, iv);

    runOnUiThread(new Runnable() {
        @Override
        public void run() {
            rootView.addView(iv);
            iv.clearFocus();
            onInitEdit(id);
        }
    });
}

public boolean focus(final int id) {
    final InvisibleInputView iv = editors.get(id);
    if (iv == null) {
        return false;
    }

    runOnUiThread(new Runnable() {
        @Override
        public void run() {
            boolean isFocused = iv.requestFocus();
            // Just Android things ;)
            if (!isFocused) {
                Log.w("darkfi", "error requesting focus for id=" + id + ": " + iv);
            }

            InputMethodManager imm = (InputMethodManager)getSystemService(Context.INPUT_METHOD_SERVICE);
            imm.showSoftInput(iv, InputMethodManager.SHOW_IMPLICIT);
        }
    });

    return true;
}

/*
public CustomInputConnection getInputConnect(int id) {
    InvisibleInputView iv = editors.get(id);
    if (iv == null) {
        return null;
    }
    return iv.inputConnection;
}
*/
public InvisibleInputView getInputView(int id) {
    return editors.get(id);
}

public boolean setText(int id, String txt) {
    InvisibleInputView iv = editors.get(id);
    if (iv == null) {
        return false;
    }

    // If inputConnection is not yet ready, then setup the editable directly.
    if (iv.inputConnection == null) {
        iv.setEditableText(txt);
        return true;
    }

    // Maybe do this on the UI thread?
    iv.inputConnection.setEditableText(txt, txt.length(), txt.length(), 0, 0);
    return true;
}
public boolean setSelection(int id, int start, int end) {
    InvisibleInputView iv = editors.get(id);
    if (iv == null) {
        return false;
    }

    // If inputConnection is not yet ready, then setup the sel directly.
    if (iv.inputConnection == null) {
        iv.setSelection(start, end);
        return true;
    }

    iv.inputConnection.beginBatchEdit();
    iv.inputConnection.setSelection(start, end);
    iv.inputConnection.endBatchEdit();
    return true;
}

/*
// Editable string with the spans displayed inline
public String getDebugEditableStr() {
    String edit = view.inputConnection.debugEditableStr();
    Log.d("darkfi", "getDebugEditableStr() -> " + edit);
    return edit;
}
*/

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

//% END

//% MAIN_ACTIVITY_ON_CREATE

rootView = layout;
editors = new HashMap<>();

view.setFocusable(false);
view.setFocusableInTouchMode(false);
view.clearFocus();

//% END

