# dnetview

A simple tui to explore darkfi ircd network topology. Lists all active
nodes, their connections and recent messages.

dnetview is based on the design-pattern Model, View, Controller. We
create a logical seperation between the underlying data structure or
Model; the ui rendering aspect which is the View; and the Controller or
game engine that makes everything run.

## Version 0.1

The current data structure or model of dnetview is this: 

### Model
```
    Mutex<HashSet<NodeId>>
    Mutex<HashMap<NodeId, NodeInfo>>
```

### View
View is a copy of the model data with additional parameters. We remove
the Mutex and add 'ListState' and 'Index' that allow us to use IdList
and InfoList as lists.

```
    IdList {
        ListState,
        HashSet<NodeId>
    }
    InfoList {
        Index,
        HashMap<NodeId, NodeInfo>
    }
```

### Controller

Inside our main function we create two parallel threads: run_rpc and render.

```
Parallel::new() {
    run_rpc(model)
    render(model)
}

```

run_rpc polls the rpc every 2 seconds. This function updates the
underlying model which is protected by mutexes, and detaches in the
background.

render takes the latest model data and updates the view in a loop.

```
loop {
    view = model.update()
    }
```

## Version 0.2:

In the first version, we could scroll the list of connected nodes and
also scroll a corresponding list of NodeInfo. In the lastest version,
the model is made more generic to allow for many types of selectable
objects: NodeInfo, SessionInfo, and ConnectInfo.

### Model

The model has 3 types of SelectableObjects which are organized around
an enum:

```
enum SelectableObject {
    Node(NodeId)
    Session(SessionId)
    Connect(ConnectId)
}
```
Id is a randomly generated number to correspond to the received info.

We copy all of these values into a info hashmap.

```
infos = Mutex<Hashmap<u32, SelectableObject>>
```

We use mutexes to protect the data because we are updating it
asynchronously.

### View

View is a copy of model without mutex's and with an index that allows the
objects to be scrollable lists.

We create a IdsList which is a hashset of all the IDs and a ListState.
This allows us to transform SelectableObject's into a scrollable list.

```
IdsList = HashSet<Id, ListState>
```

We also create a function called render() that draws each window.

```
NodeInfo.render()
SessionInfo.render()
ConnectInfo.render()
```

### Controller

Like the previous version, we have two functions that run in parallel:
run_rpc() and render().  In run_rpc, we poll the rpc and write the new
values to the model:


```
    info = NodeInfo::new()...
    session = SessionInfo::new()...
    connection = ConnectInfo::new()...
    model_vec.push(info, session, connection)
```

We then continuously update the view with the new data from the model.

```
loop {
    view = model.update()
}
```
